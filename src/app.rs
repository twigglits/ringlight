// COSMIC applet implementation for Ringlight.
//
// Architecture:
//   - Panel icon button in the COSMIC panel bar
//   - Popup window with brightness / color-temp / size / preset controls
//   - Four transparent layer-shell surfaces (one per screen edge) for the glow
//   - Background subscriptions for camera monitoring and mouse tracking

use crate::glow::GlowProgram;
use crate::settings::{GlowSize, HoleSize, RingLightSettings};
use cosmic::app::{Core, Task};
use cosmic::iced_widget::shader::Shader;
use futures_util::FutureExt;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Limits, Subscription};
use cosmic::widget;
use cosmic::{Application, Element};
use std::time::{Duration, Instant};

// Layer-surface commands and types.
// Verified paths for the pop-os/iced fork used by libcosmic:
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity, Layer,
};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;

const APP_ID: &str = "com.github.twigglits.ringlight";

pub struct RingLight {
    core: Core,
    popup: Option<Id>,
    settings: RingLightSettings,
    camera_active: bool,
    overlay_id: Option<Id>,
    cursor: crate::cursor::CursorState,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    ToggleEnabled(bool),
    ToggleAutoMode(bool),
    SetBrightness(f32),
    SetColorTemp(f32),
    SetGlowSize(GlowSize),
    SetHoleSize(HoleSize),
    CameraStateChanged(bool),
    CursorMoved(crate::cursor::CursorState),
    ApplyPreset(&'static str),
    PersistSettings,
    ReapplyInputZone,
}

/// Re-send an empty input region for an existing layer surface.
///
/// libcosmic exposes no public command for this (only `set_layer`, `set_size`
/// and friends), so the internal action is constructed directly, mirroring how
/// those commands are built.
fn set_empty_input_zone(id: Id) -> Task<Message> {
    use cosmic::iced_runtime::platform_specific::{self, wayland};
    cosmic::iced_runtime::task::effect(cosmic::iced_runtime::Action::PlatformSpecific(
        platform_specific::Action::Wayland(wayland::Action::LayerSurface(
            wayland::layer_surface::Action::InputZone {
                id,
                zone: Some(Vec::new()),
            },
        )),
    ))
}

impl Application for RingLight {
    type Message = Message;
    type Executor = cosmic::executor::Default;
    type Flags = ();

    const APP_ID: &'static str = APP_ID;

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let app = Self {
            core,
            popup: None,
            settings: crate::config::load(),
            camera_active: false,
            overlay_id: None,
            cursor: crate::cursor::CursorState::default(),
        };
        (app, Task::none())
    }

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    // -- Panel icon ----------------------------------------------------------

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("display-brightness-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    // -- Popup / overlay views -----------------------------------------------

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if self.popup == Some(id) {
            return self.popup_view();
        }
        if self.overlay_id == Some(id) {
            return self.overlay_view();
        }
        widget::text("").into()
    }

    // -- Update --------------------------------------------------------------

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    settings.positioner.size_limits = Limits::NONE
                        .max_width(340.0)
                        .min_width(280.0)
                        .min_height(200.0)
                        .max_height(600.0);
                    get_popup(settings)
                };
            }

            Message::ToggleEnabled(on) => {
                self.settings.enabled = on;
                crate::config::save(&self.settings);
                return self.sync_overlay();
            }
            Message::ToggleAutoMode(on) => {
                self.settings.auto_mode = on;
                if on {
                    self.settings.enabled = self.camera_active;
                }
                crate::config::save(&self.settings);
                return self.sync_overlay();
            }

            Message::SetBrightness(b) => {
                self.settings.brightness = b;
            }
            Message::SetColorTemp(t) => {
                self.settings.color_temp = t;
            }
            Message::SetGlowSize(s) => {
                self.settings.glow_size = s;
                crate::config::save(&self.settings);
            }
            Message::SetHoleSize(s) => {
                self.settings.hole_size = s;
                crate::config::save(&self.settings);
            }

            Message::CameraStateChanged(active) => {
                self.camera_active = active;
                if self.settings.auto_mode {
                    self.settings.enabled = active;
                    return self.sync_overlay();
                }
            }

            Message::CursorMoved(c) => {
                self.cursor = c;
            }

            Message::ApplyPreset(name) => {
                match name {
                    "warm" => {
                        self.settings.brightness = 0.5;
                        self.settings.color_temp = 0.1;
                        self.settings.glow_size = GlowSize::Small;
                    }
                    "cool" => {
                        self.settings.brightness = 0.8;
                        self.settings.color_temp = 0.9;
                        self.settings.glow_size = GlowSize::Medium;
                    }
                    "subtle" => {
                        self.settings.brightness = 0.3;
                        self.settings.color_temp = 0.5;
                        self.settings.glow_size = GlowSize::Small;
                    }
                    "bright" => {
                        self.settings.brightness = 1.0;
                        self.settings.color_temp = 0.5;
                        self.settings.glow_size = GlowSize::Large;
                    }
                    _ => {}
                }
                crate::config::save(&self.settings);
            }

            Message::PersistSettings => {
                crate::config::save(&self.settings);
            }

            Message::ReapplyInputZone => {
                if let Some(id) = self.overlay_id {
                    return set_empty_input_zone(id);
                }
            }

        }
        Task::none()
    }

    // -- Subscriptions -------------------------------------------------------

    fn subscription(&self) -> Subscription<Self::Message> {
        let camera_sub = Subscription::run(|| {
            async {
                let (tx, rx) = tokio::sync::mpsc::channel::<bool>(10);
                tokio::spawn(crate::camera::monitor_camera(tx));
                futures_util::stream::unfold(rx, |mut rx| async {
                    rx.recv().await.map(|active| (Message::CameraStateChanged(active), rx))
                })
            }
            .flatten_stream()
        });

        // The watch channel coalesces, and this throttles further: at most one
        // message per frame, and nothing at all for sub-pixel movement. The
        // old code emitted one message per raw input event and repainted four
        // surfaces for each.
        let cursor_sub = Subscription::run(|| {
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
                                > 0.0005; // ~1.5px on a 3000px-wide output
                            let toggled = now.visible != last.visible;

                            if toggled
                                || (moved && last_emit.elapsed() >= Duration::from_millis(16))
                            {
                                last_emit = Instant::now();
                                last = now;
                                return Some((Message::CursorMoved(now), (rx, last_emit, last)));
                            }
                        }
                    },
                )
            }
            .flatten_stream()
        });

        let mut subs = vec![camera_sub, cursor_sub];

        // Keep re-asserting the empty input region while the overlay exists.
        //
        // iced sets the input region once, before the layer surface is mapped,
        // and cosmic-comp discards it: the surface then accepts pointer input
        // across the whole screen and swallows every click. Re-applying after
        // the surface is mapped makes it stick. Verified by instrumenting the
        // shader's event handler -- 225 stolen mouse events before, 0 after.
        //
        // This repeats rather than firing once because there is no "mapped"
        // event to hook, and it is self-healing if the region is ever dropped
        // again (output change, remap). One tiny Wayland request per second,
        // and only while the glow is actually on.
        if self.is_active() {
            subs.push(Subscription::run(|| {
                async {
                    let interval = tokio::time::interval(Duration::from_secs(1));
                    futures_util::stream::unfold(interval, |mut i| async move {
                        i.tick().await;
                        Some((Message::ReapplyInputZone, i))
                    })
                }
                .flatten_stream()
            }));
        }

        Subscription::batch(subs)
    }
}

// ===========================================================================
// Private helpers
// ===========================================================================

impl RingLight {
    fn is_active(&self) -> bool {
        self.settings.enabled || (self.settings.auto_mode && self.camera_active)
    }

    // -- Overlay surface lifecycle -------------------------------------------

    /// Ensure overlay surfaces match the current enabled state.
    fn sync_overlay(&mut self) -> Task<Message> {
        if self.is_active() {
            self.create_overlay()
        } else {
            self.destroy_overlay()
        }
    }

    fn create_overlay(&mut self) -> Task<Message> {
        if self.overlay_id.is_some() {
            return Task::none();
        }
        let id = Id::unique();
        self.overlay_id = Some(id);

        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: "ringlight".to_string(),
            // Top, not Overlay: the glow belongs above windows but below the
            // OSD and lock screen.
            layer: Layer::Top,
            // Anchored to both opposite edge pairs, so the compositor stretches
            // the surface across the whole output regardless of `size`.
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            size: Some((None, None)),
            // -1, not 0: 0 means "move me so I don't occlude the panel", which
            // pushes the glow inward off the true screen edge.
            exclusive_zone: -1,
            // Empty input region => no pointer input at all => click-through.
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

    fn destroy_overlay(&mut self) -> Task<Message> {
        match self.overlay_id.take() {
            Some(id) => destroy_layer_surface(id),
            None => Task::none(),
        }
    }

    // -- View builders -------------------------------------------------------

    fn overlay_view(&self) -> Element<'_, Message> {
        Shader::new(GlowProgram {
            color: self.settings.glow_color(),
            brightness: self.settings.brightness,
            glow_fraction: self.settings.glow_fraction(),
            hole_fraction: if self.cursor.visible {
                self.settings.hole_fraction()
            } else {
                0.0
            },
            cursor: self.cursor.pos,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn popup_view(&self) -> Element<'_, Message> {
        let active = self.is_active();
        let status_text = if active { "Active" } else { "Inactive" };
        let camera_text = if self.camera_active {
            "Camera: In use"
        } else {
            "Camera: Off"
        };

        let content = widget::column::with_capacity(18)
            .spacing(8)
            .padding(16)
            // Header
            .push(widget::text::title4("Ringlight"))
            .push(widget::text::body(status_text))
            .push(widget::text::caption(camera_text))
            .push(widget::divider::horizontal::default())
            // Toggles
            .push(
                widget::row::with_capacity(2)
                    .align_y(Alignment::Center)
                    .push(widget::text::body("Enabled"))
                    .push(widget::Space::new().width(Length::Fill))
                    .push(widget::toggler(self.settings.enabled).on_toggle(Message::ToggleEnabled)),
            )
            .push(
                widget::row::with_capacity(2)
                    .align_y(Alignment::Center)
                    .push(widget::text::body("Auto (camera)"))
                    .push(widget::Space::new().width(Length::Fill))
                    .push(
                        widget::toggler(self.settings.auto_mode).on_toggle(Message::ToggleAutoMode),
                    ),
            )
            .push(widget::divider::horizontal::default())
            // Brightness
            .push(widget::text::body(format!(
                "Brightness: {:.0}%",
                self.settings.brightness * 100.0
            )))
            .push(
                widget::slider(0.0..=1.0, self.settings.brightness, Message::SetBrightness)
                    .step(0.05)
                    .on_release(Message::PersistSettings),
            )
            // Color temperature
            .push(widget::text::body(format!(
                "Color: {}",
                if self.settings.color_temp < 0.5 {
                    "Warm"
                } else {
                    "Cool"
                }
            )))
            .push(
                widget::slider(0.0..=1.0, self.settings.color_temp, Message::SetColorTemp)
                    .step(0.05)
                    .on_release(Message::PersistSettings),
            )
            .push(widget::divider::horizontal::default())
            // Glow size
            .push(widget::text::body("Glow Size"))
            .push(
                widget::row::with_capacity(3)
                    .spacing(8)
                    .push(size_btn("S", GlowSize::Small, self.settings.glow_size))
                    .push(size_btn("M", GlowSize::Medium, self.settings.glow_size))
                    .push(size_btn("L", GlowSize::Large, self.settings.glow_size)),
            )
            // Cursor hole
            .push(widget::text::body("Cursor Hole"))
            .push(
                widget::row::with_capacity(4)
                    .spacing(8)
                    .push(hole_btn("Off", HoleSize::Off, self.settings.hole_size))
                    .push(hole_btn("S", HoleSize::Small, self.settings.hole_size))
                    .push(hole_btn("M", HoleSize::Medium, self.settings.hole_size))
                    .push(hole_btn("L", HoleSize::Large, self.settings.hole_size)),
            )
            .push(widget::divider::horizontal::default())
            // Presets
            .push(widget::text::body("Presets"))
            .push(
                widget::row::with_capacity(4)
                    .spacing(8)
                    .push(widget::button::text("Warm").on_press(Message::ApplyPreset("warm")))
                    .push(widget::button::text("Cool").on_press(Message::ApplyPreset("cool")))
                    .push(widget::button::text("Subtle").on_press(Message::ApplyPreset("subtle")))
                    .push(widget::button::text("Bright").on_press(Message::ApplyPreset("bright"))),
            );

        self.core.applet.popup_container(content).into()
    }
}

// ---------------------------------------------------------------------------
// Widget helpers
// ---------------------------------------------------------------------------

fn size_btn(label: &str, size: GlowSize, current: GlowSize) -> Element<'_, Message> {
    let mut btn = widget::button::text(label).on_press(Message::SetGlowSize(size));
    if size == current {
        btn = btn.class(cosmic::theme::Button::Suggested);
    }
    btn.into()
}

fn hole_btn(label: &str, size: HoleSize, current: HoleSize) -> Element<'_, Message> {
    let mut btn = widget::button::text(label).on_press(Message::SetHoleSize(size));
    if size == current {
        btn = btn.class(cosmic::theme::Button::Suggested);
    }
    btn.into()
}

