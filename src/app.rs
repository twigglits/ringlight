// COSMIC applet implementation for Ringlight.
//
// Architecture:
//   - Panel icon button in the COSMIC panel bar
//   - Popup window with brightness / color-temp / size / preset controls
//   - A child process that owns the glow surface (see ipc.rs for why it cannot
//     live here) and is fed settings over its stdin
//   - Background subscription for camera monitoring
//
// The cursor is tracked by the overlay process, not here: it is the only part
// that needs it, and doing it there saves this process a Wayland connection.

use crate::ipc::OverlayProcess;
use crate::settings::{GlowSize, HoleSize};
use cosmic::app::{Core, Task};
use futures_util::FutureExt;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Limits, Subscription};
use cosmic::widget;
use cosmic::{Application, Element};

const APP_ID: &str = "com.github.twigglits.ringlight";

pub struct RingLight {
    core: Core,
    popup: Option<Id>,
    settings: crate::settings::RingLightSettings,
    camera_active: bool,
    overlay: Option<OverlayProcess>,
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
    ApplyPreset(&'static str),
    PersistSettings,
}

impl Application for RingLight {
    type Message = Message;
    type Executor = cosmic::executor::Default;
    type Flags = ();

    const APP_ID: &'static str = APP_ID;

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let mut app = Self {
            core,
            popup: None,
            settings: crate::config::load(),
            camera_active: false,
            overlay: None,
        };
        // Persisted `enabled` has to be honoured here: nothing else starts the
        // overlay until a toggle or a camera event arrives, so without this the
        // applet starts up claiming to be on with no glow.
        app.sync_overlay();
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
                self.sync_overlay();
            }
            Message::ToggleAutoMode(on) => {
                self.settings.auto_mode = on;
                if on {
                    self.settings.enabled = self.camera_active;
                }
                crate::config::save(&self.settings);
                self.sync_overlay();
            }

            Message::SetBrightness(b) => {
                self.settings.brightness = b;
                self.push_settings();
            }
            Message::SetColorTemp(t) => {
                self.settings.color_temp = t;
                self.push_settings();
            }
            Message::SetGlowSize(s) => {
                self.settings.glow_size = s;
                crate::config::save(&self.settings);
                self.push_settings();
            }
            Message::SetHoleSize(s) => {
                self.settings.hole_size = s;
                crate::config::save(&self.settings);
                self.push_settings();
            }

            Message::CameraStateChanged(active) => {
                self.camera_active = active;
                if self.settings.auto_mode {
                    self.settings.enabled = active;
                    self.sync_overlay();
                }
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
                self.push_settings();
            }

            Message::PersistSettings => {
                crate::config::save(&self.settings);
                self.push_settings();
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

        camera_sub
    }
}

// ===========================================================================
// Private helpers
// ===========================================================================

impl RingLight {
    fn is_active(&self) -> bool {
        self.settings.enabled || (self.settings.auto_mode && self.camera_active)
    }

    // -- Overlay process lifecycle -------------------------------------------

    /// Ensure the overlay process matches the current enabled state.
    fn sync_overlay(&mut self) {
        if self.is_active() {
            self.start_overlay();
        } else {
            // Drop kills the child, so the glow goes with it.
            self.overlay = None;
        }
    }

    fn start_overlay(&mut self) {
        if self.overlay.is_some() {
            return;
        }
        // Launching our own executable rather than a looked-up name keeps the
        // two halves the same build even for an uninstalled binary.
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                log::warn!("ringlight: cannot locate own executable, no glow: {e}");
                return;
            }
        };
        match OverlayProcess::spawn(crate::ipc::overlay_command(&exe)) {
            Ok(mut child) => {
                // Send immediately: the child boots from the config file, which
                // is stale for anything not yet persisted (a mid-drag slider).
                child.send(&self.settings);
                self.overlay = Some(child);
            }
            Err(e) => log::warn!("ringlight: could not start overlay process: {e}"),
        }
    }

    /// Push current settings to the overlay process, if one is running.
    fn push_settings(&mut self) {
        let broken = match self.overlay.as_mut() {
            Some(child) => !child.send(&self.settings),
            None => false,
        };
        if broken {
            log::warn!("ringlight: overlay process went away");
            self.overlay = None;
        }
    }

    // -- View builders -------------------------------------------------------

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

