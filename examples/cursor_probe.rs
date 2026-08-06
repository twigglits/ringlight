// Probe: can an unprivileged Wayland client read the global cursor position on
// cosmic-comp via ext-image-copy-capture-v1's pointer cursor session?
//
// Verifies three things:
//   1. cosmic-comp advertises ext_output_image_capture_source_manager_v1 and
//      ext_image_copy_capture_manager_v1 to a plain client (no portal, no perms).
//   2. create_pointer_cursor_session succeeds.
//   3. `position` events actually arrive on cursor motion WITHOUT ever
//      attaching a buffer or capturing a frame.

use std::time::{Duration, Instant};
use wayland_client::{
    protocol::{
        wl_output::WlOutput,
        wl_pointer::WlPointer,
        wl_registry,
        wl_seat::{self, WlSeat},
    },
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_cursor_session_v1::{self, ExtImageCopyCaptureCursorSessionV1},
    ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
};

#[derive(Default)]
struct State {
    globals: Vec<String>,
    output: Option<WlOutput>,
    seat: Option<WlSeat>,
    has_pointer: bool,
    source_mgr: Option<ExtOutputImageCaptureSourceManagerV1>,
    capture_mgr: Option<ExtImageCopyCaptureManagerV1>,
    positions: Vec<(i32, i32)>,
    hotspots: Vec<(i32, i32)>,
    enters: u32,
    leaves: u32,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global { name, interface, version } = event else {
            return;
        };
        state.globals.push(interface.clone());
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
        if let wl_seat::Event::Capabilities {
            capabilities: wayland_client::WEnum::Value(caps),
        } = event
        {
            state.has_pointer = caps.contains(wl_seat::Capability::Pointer);
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
            Event::Enter => {
                state.enters += 1;
                println!("  [event] enter");
            }
            Event::Leave => {
                state.leaves += 1;
                println!("  [event] leave");
            }
            Event::Position { x, y } => {
                state.positions.push((x, y));
                println!("  [event] position x={x} y={y}");
            }
            Event::Hotspot { x, y } => {
                state.hotspots.push((x, y));
                println!("  [event] hotspot x={x} y={y}");
            }
            _ => println!("  [event] (other)"),
        }
    }
}

wayland_client::delegate_noop!(State: ignore WlOutput);
wayland_client::delegate_noop!(State: ignore WlPointer);
wayland_client::delegate_noop!(State: ExtOutputImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(State: ExtImageCopyCaptureManagerV1);
wayland_client::delegate_noop!(State: ignore ExtImageCaptureSourceV1);

fn main() {
    let conn = Connection::connect_to_env().expect("connect to wayland");
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());

    let mut state = State::default();
    queue.roundtrip(&mut state).expect("registry roundtrip");
    queue.roundtrip(&mut state).expect("seat caps roundtrip");

    println!("=== STEP 1: required globals ===");
    for want in [
        "ext_output_image_capture_source_manager_v1",
        "ext_image_copy_capture_manager_v1",
        "wl_seat",
        "wl_output",
    ] {
        println!(
            "  {:<48} {}",
            want,
            if state.globals.iter().any(|g| g == want) { "PRESENT" } else { "MISSING" }
        );
    }
    println!("  (seat has pointer capability: {})", state.has_pointer);

    let (Some(source_mgr), Some(capture_mgr), Some(output), Some(seat)) = (
        state.source_mgr.clone(),
        state.capture_mgr.clone(),
        state.output.clone(),
        state.seat.clone(),
    ) else {
        println!("\nRESULT: FAIL - required globals unavailable");
        return;
    };
    if !state.has_pointer {
        println!("\nRESULT: FAIL - seat has no pointer capability");
        return;
    }

    println!("\n=== STEP 2: create source + pointer cursor session ===");
    let pointer = seat.get_pointer(&qh, ());
    let source = source_mgr.create_source(&output, &qh, ());
    let _session = capture_mgr.create_pointer_cursor_session(&source, &pointer, &qh, ());

    if let Err(e) = queue.roundtrip(&mut state) {
        println!("  roundtrip FAILED: {e}");
        println!("\nRESULT: FAIL - session creation rejected by compositor");
        return;
    }
    println!("  session created, no protocol error");

    println!("\n=== STEP 3: listening 15s for position events (MOVE YOUR MOUSE) ===");
    println!("  NOTE: no buffer attached, no frame captured -- position only.");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if queue.blocking_dispatch(&mut state).is_err() {
            println!("  dispatch error (connection lost)");
            break;
        }
        conn.flush().ok();
        if state.positions.len() > 40 {
            println!("  (plenty of samples, stopping early)");
            break;
        }
    }

    println!("\n=== RESULT ===");
    println!("  enter events:    {}", state.enters);
    println!("  leave events:    {}", state.leaves);
    println!("  hotspot events:  {}", state.hotspots.len());
    println!("  position events: {}", state.positions.len());
    if let (Some(first), Some(last)) = (state.positions.first(), state.positions.last()) {
        println!("  first position:  {first:?}");
        println!("  last  position:  {last:?}");
    }
    if state.positions.len() >= 2 {
        println!("\n  VERDICT: WORKS - global cursor position readable without input grab.");
    } else if state.enters > 0 || !state.positions.is_empty() {
        println!("\n  VERDICT: PARTIAL - session live but few/no position updates.");
    } else {
        println!("\n  VERDICT: NO EVENTS - session created but compositor sent nothing.");
    }
}
