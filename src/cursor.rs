//! Global cursor position via cosmic-comp's ext-image-copy-capture-v1.
//!
//! The pointer cursor session reports position independently of frame
//! capture, so this needs no permissions, never grabs input, attaches no
//! buffers, and captures no frames. It replaces the /dev/input reader, which
//! needed `input` group membership and drifted because it accumulated raw
//! relative deltas with no pointer acceleration applied.
//!
//! Positions are published NORMALIZED to [0,1] so nothing downstream has to
//! know the output scale -- which is what made the old code wrong on this
//! 200%-scaled display.

use std::time::Duration;
use tokio::sync::watch;

/// Convert a compositor cursor position into a normalized [0,1] coordinate.
///
/// `buffer` is the output's physical size; `swap_axes` is true for 90/270
/// degree transforms, where the buffer's extents are exchanged.
pub fn normalize(x: i32, y: i32, buffer: (i32, i32), swap_axes: bool) -> [f32; 2] {
    let (w, h) = if swap_axes { (buffer.1, buffer.0) } else { buffer };
    if w <= 0 || h <= 0 {
        return [0.5, 0.5];
    }
    [
        (x as f32 / w as f32).clamp(0.0, 1.0),
        (y as f32 / h as f32).clamp(0.0, 1.0),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorState {
    /// Normalized [0,1] position within the output.
    pub pos: [f32; 2],
    /// False when the cursor has left the output or tracking is unavailable.
    pub visible: bool,
}

impl Default for CursorState {
    fn default() -> Self {
        Self { pos: [0.5, 0.5], visible: false }
    }
}

use wayland_client::{
    protocol::{
        wl_output::{self, WlOutput},
        wl_pointer::WlPointer,
        wl_registry,
        wl_seat::{self, WlSeat},
    },
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_cursor_session_v1::{self, ExtImageCopyCaptureCursorSessionV1},
    ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
};

enum SessionError {
    /// The compositor does not offer what we need. Retrying will not help.
    Unsupported(&'static str),
    /// The connection died. Retrying may help.
    Transient(String),
}

struct State {
    tx: watch::Sender<CursorState>,
    output: Option<WlOutput>,
    seat: Option<WlSeat>,
    has_pointer: bool,
    source_mgr: Option<ExtOutputImageCaptureSourceManagerV1>,
    capture_mgr: Option<ExtImageCopyCaptureManagerV1>,
    buffer: Option<(i32, i32)>,
    swap_axes: bool,
    visible: bool,
}

impl State {
    fn publish(&self, pos: [f32; 2]) {
        let _ = self.tx.send(CursorState { pos, visible: self.visible });
    }
}

/// Start cursor tracking. Returns immediately; the receiver holds
/// `CursorState::default()` until the first position arrives.
pub fn start() -> watch::Receiver<CursorState> {
    let (tx, rx) = watch::channel(CursorState::default());

    std::thread::spawn(move || {
        let mut backoff = Duration::from_millis(250);
        loop {
            match run_session(&tx) {
                Err(SessionError::Unsupported(what)) => {
                    log::warn!(
                        "ringlight: cursor tracking unavailable ({what}); \
                         the glow will render without a cursor hole"
                    );
                    let _ = tx.send(CursorState { pos: [0.5, 0.5], visible: false });
                    return; // Will not become available later.
                }
                Err(SessionError::Transient(e)) => {
                    log::warn!("ringlight: cursor session lost: {e}; retrying");
                }
                Ok(()) => {
                    log::warn!("ringlight: cursor session ended; retrying");
                }
            }

            if tx.send(CursorState { pos: [0.5, 0.5], visible: false }).is_err() {
                return; // Application gone.
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });

    rx
}

fn run_session(tx: &watch::Sender<CursorState>) -> Result<(), SessionError> {
    // Our own connection, deliberately separate from the one iced-sctk owns.
    let conn = Connection::connect_to_env()
        .map_err(|e| SessionError::Transient(format!("connect failed: {e}")))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());

    let mut state = State {
        tx: tx.clone(),
        output: None,
        seat: None,
        has_pointer: false,
        source_mgr: None,
        capture_mgr: None,
        buffer: None,
        swap_axes: false,
        visible: false,
    };

    // First roundtrip binds globals; the second delivers seat capabilities
    // and the output mode.
    queue.roundtrip(&mut state).map_err(|e| SessionError::Transient(e.to_string()))?;
    queue.roundtrip(&mut state).map_err(|e| SessionError::Transient(e.to_string()))?;

    let source_mgr = state
        .source_mgr
        .clone()
        .ok_or(SessionError::Unsupported("no ext_output_image_capture_source_manager_v1"))?;
    let capture_mgr = state
        .capture_mgr
        .clone()
        .ok_or(SessionError::Unsupported("no ext_image_copy_capture_manager_v1"))?;
    let output = state.output.clone().ok_or(SessionError::Unsupported("no wl_output"))?;
    let seat = state.seat.clone().ok_or(SessionError::Unsupported("no wl_seat"))?;
    if !state.has_pointer {
        return Err(SessionError::Unsupported("seat has no pointer"));
    }

    let pointer = seat.get_pointer(&qh, ());
    let source = source_mgr.create_source(&output, &qh, ());
    // No buffer is ever attached and no frame is ever captured: position
    // events are independent of the capture pipeline.
    let _session = capture_mgr.create_pointer_cursor_session(&source, &pointer, &qh, ());

    queue.roundtrip(&mut state).map_err(|e| SessionError::Transient(e.to_string()))?;
    log::info!("ringlight: cursor tracking active via ext-image-copy-capture");

    loop {
        queue
            .blocking_dispatch(&mut state)
            .map_err(|e| SessionError::Transient(e.to_string()))?;
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global { name, interface, version } = event else {
            return;
        };
        match interface.as_str() {
            "wl_output" if state.output.is_none() => {
                state.output = Some(registry.bind(name, version.min(4), qh, ()));
            }
            "wl_seat" if state.seat.is_none() => {
                state.seat = Some(registry.bind(name, version.min(7), qh, ()));
            }
            "ext_output_image_capture_source_manager_v1" => {
                state.source_mgr = Some(registry.bind(name, 1, qh, ()));
            }
            "ext_image_copy_capture_manager_v1" => {
                state.capture_mgr = Some(registry.bind(name, 1, qh, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event {
            state.has_pointer = caps.contains(wl_seat::Capability::Pointer);
        }
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Mode { flags, width, height, .. } => {
                let current =
                    matches!(flags, WEnum::Value(f) if f.contains(wl_output::Mode::Current));
                if current {
                    state.buffer = Some((width, height));
                }
            }
            wl_output::Event::Geometry { transform, .. } => {
                state.swap_axes = matches!(
                    transform,
                    WEnum::Value(wl_output::Transform::_90)
                        | WEnum::Value(wl_output::Transform::_270)
                        | WEnum::Value(wl_output::Transform::Flipped90)
                        | WEnum::Value(wl_output::Transform::Flipped270)
                );
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureCursorSessionV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureCursorSessionV1,
        event: ext_image_copy_capture_cursor_session_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_image_copy_capture_cursor_session_v1::Event;
        match event {
            Event::Enter => state.visible = true,
            Event::Leave => {
                // Suppress the hole rather than strand it at a stale position.
                state.visible = false;
                let pos = state.tx.borrow().pos;
                state.publish(pos);
            }
            Event::Position { x, y } => {
                let Some(buffer) = state.buffer else { return };
                state.publish(normalize(x, y, buffer, state.swap_axes));
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(State: ignore WlPointer);
wayland_client::delegate_noop!(State: ExtOutputImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(State: ExtImageCopyCaptureManagerV1);
wayland_client::delegate_noop!(State: ignore ExtImageCaptureSourceV1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_against_the_buffer_size() {
        // Centre of a 3000x2000 output.
        assert_eq!(normalize(1500, 1000, (3000, 2000), false), [0.5, 0.5]);
        // Origin and far corner.
        assert_eq!(normalize(0, 0, (3000, 2000), false), [0.0, 0.0]);
        assert_eq!(normalize(3000, 2000, (3000, 2000), false), [1.0, 1.0]);
    }

    #[test]
    fn swaps_axes_for_rotated_outputs() {
        // A 90-degree transform means the buffer's logical extent is swapped.
        assert_eq!(normalize(1000, 1500, (3000, 2000), true), [0.5, 0.5]);
    }

    #[test]
    fn clamps_out_of_range_positions() {
        assert_eq!(normalize(-50, -50, (3000, 2000), false), [0.0, 0.0]);
        assert_eq!(normalize(9999, 9999, (3000, 2000), false), [1.0, 1.0]);
    }

    #[test]
    fn degenerate_buffer_does_not_divide_by_zero() {
        let p = normalize(10, 10, (0, 0), false);
        assert!(p[0].is_finite() && p[1].is_finite());
    }
}
