//! The overlay process: a windowless daemon whose only surface is the glow.
//!
//! This is deliberately *not* a `cosmic::applet` and not even a
//! `cosmic::app::run`. Both insist on a main window — libcosmic's `Settings`
//! keeps `no_main_window` private — and a stray toplevel is unacceptable for
//! something whose whole job is to be invisible furniture. `iced::daemon`
//! starts with no windows at all, so the layer surface created during boot is
//! the only thing this process ever puts on screen.
//!
//! It exists as a separate process because the applet's Wayland connection
//! comes from cosmic-panel and does not honour an empty input region; see
//! `ipc.rs` and CLAUDE.md.

use crate::glow::GlowProgram;
use crate::settings::RingLightSettings;
use cosmic::iced;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    get_layer_surface, Anchor, KeyboardInteractivity, Layer,
};
use cosmic::iced::window::Id;
use cosmic::iced::{Element, Length, Limits, Subscription, Task, Theme};
use cosmic::iced_widget::shader::Shader;
use futures_util::FutureExt;
use std::time::{Duration, Instant};

struct Overlay {
    settings: RingLightSettings,
    cursor: crate::cursor::CursorState,
}

#[derive(Debug, Clone)]
enum Message {
    /// New settings pushed by the applet over stdin.
    Settings(Box<RingLightSettings>),
    CursorMoved(crate::cursor::CursorState),
    /// The applet closed the pipe, or died.
    ParentGone,
}

/// The surface must not be painted at all before the glow is drawn.
///
/// A daemon fills every surface with its theme's base background first, and
/// iced's default themes are opaque. Inheriting that turns a full-screen
/// click-through overlay into a full-screen white wash that hides the desktop
/// completely. The applet half avoids this via `cosmic::applet::style()`;
/// this is the same thing for a plain iced daemon.
fn surface_style() -> iced::theme::Style {
    iced::theme::Style {
        background_color: iced::Color::TRANSPARENT,
        // Nothing in this surface draws text or icons; these just have to be
        // something.
        text_color: iced::Color::WHITE,
        icon_color: iced::Color::WHITE,
    }
}

/// Run the overlay process. Returns when the applet's pipe closes.
pub fn run() -> iced::Result {
    iced::daemon(boot, update, view)
        .title(|_state: &Overlay, _id| "ringlight-overlay".to_string())
        .style(|_state, _theme| surface_style())
        .subscription(subscription)
        .run()
}

fn boot() -> (Overlay, Task<Message>) {
    let overlay = Overlay {
        // The applet streams the authoritative values immediately; reading the
        // config here only avoids one frame of default-coloured glow.
        settings: crate::config::load(),
        cursor: crate::cursor::CursorState::default(),
    };
    (overlay, create_surface())
}

fn create_surface() -> Task<Message> {
    get_layer_surface(SctkLayerSurfaceSettings {
        id: Id::unique(),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: "ringlight".to_string(),
        // Top, not Overlay: the glow belongs above windows but below the OSD
        // and lock screen.
        layer: Layer::Top,
        // Anchored to both opposite edge pairs, so the compositor stretches the
        // surface across the whole output regardless of `size`.
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        size: Some((None, None)),
        // -1, not 0: 0 means "move me so I don't occlude the panel", which
        // pushes the glow inward off the true screen edge.
        exclusive_zone: -1,
        // Empty input region => no pointer input at all => click-through. This
        // is honoured here, on a direct session connection, and is exactly what
        // cosmic-panel's connection failed to honour.
        input_zone: Some(Vec::new()),
        // The default max is 1920x1080, which would clamp larger screens.
        size_limits: Limits::NONE
            .min_width(1.0)
            .min_height(1.0)
            .max_width(16384.0)
            .max_height(16384.0),
        ..Default::default()
    })
}

fn update(state: &mut Overlay, message: Message) -> Task<Message> {
    match message {
        Message::Settings(settings) => {
            state.settings = *settings;
            Task::none()
        }
        Message::CursorMoved(cursor) => {
            state.cursor = cursor;
            Task::none()
        }
        // Leaving a full-screen surface up with nothing driving it would be
        // worse than no glow at all.
        Message::ParentGone => iced::exit(),
    }
}

fn view(state: &Overlay, _id: Id) -> Element<'_, Message, Theme> {
    Shader::new(GlowProgram {
        color: state.settings.glow_color(),
        brightness: state.settings.brightness,
        glow_fraction: state.settings.glow_fraction(),
        hole_fraction: if state.cursor.visible {
            state.settings.hole_fraction()
        } else {
            0.0
        },
        cursor: state.cursor.pos,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn subscription(_state: &Overlay) -> Subscription<Message> {
    Subscription::batch(vec![settings_sub(), cursor_sub()])
}

/// Settings arrive as one JSON object per line on stdin. EOF means the applet
/// is gone, which is the shutdown signal.
fn settings_sub() -> Subscription<Message> {
    Subscription::run(|| {
        async {
            let (tx, rx) = tokio::sync::mpsc::channel::<Message>(8);
            // Blocking line reads belong on their own thread, not on the
            // executor that is also driving the compositor connection.
            std::thread::spawn(move || {
                use std::io::BufRead;
                let stdin = std::io::stdin();
                for line in stdin.lock().lines() {
                    let Ok(line) = line else { break };
                    if let Some(settings) = crate::ipc::decode(line.trim()) {
                        if tx.blocking_send(Message::Settings(Box::new(settings))).is_err() {
                            return;
                        }
                    }
                }
                let _ = tx.blocking_send(Message::ParentGone);
            });
            futures_util::stream::unfold(rx, |mut rx| async {
                rx.recv().await.map(|message| (message, rx))
            })
        }
        .flatten_stream()
    })
}

/// Same throttling as the applet used to do: at most one message per frame, and
/// nothing at all for sub-pixel movement.
fn cursor_sub() -> Subscription<Message> {
    Subscription::run(|| {
        async {
            let rx = crate::cursor::start();
            let init = *rx.borrow();
            futures_util::stream::unfold(
                (rx, Instant::now(), init),
                |(mut rx, mut last_emit, mut last)| async move {
                    loop {
                        rx.changed().await.ok()?;
                        let now = *rx.borrow();

                        let moved = (now.pos[0] - last.pos[0]).abs()
                            + (now.pos[1] - last.pos[1]).abs()
                            > 0.0005;
                        let toggled = now.visible != last.visible;

                        if toggled || (moved && last_emit.elapsed() >= Duration::from_millis(16)) {
                            last_emit = Instant::now();
                            last = now;
                            return Some((Message::CursorMoved(now), (rx, last_emit, last)));
                        }
                    }
                },
            )
        }
        .flatten_stream()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_surface_is_never_painted_opaque() {
        // Regression: the first cut inherited iced's default Light theme, whose
        // base background is opaque, and whited out the entire display.
        assert_eq!(
            surface_style().background_color.a,
            0.0,
            "an opaque background hides the whole desktop"
        );
    }
}
