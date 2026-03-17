// COSMIC applet implementation for Ringlight.
//
// Architecture:
//   - Panel icon button in the COSMIC panel bar
//   - Popup window with brightness / color-temp / size / preset controls
//   - Four transparent layer-shell surfaces (one per screen edge) for the glow
//   - Background subscriptions for camera monitoring and mouse tracking

use crate::overlay::{self, EdgeSide, GlowCache, GlowProgram};
use crate::settings::{GlowSize, HoleSize, RingLightSettings};
use cosmic::app::{Core, Task};
use futures_util::FutureExt;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{widget::canvas::Canvas, Alignment, Length, Limits, Subscription};
use cosmic::widget;
use cosmic::{Application, Element};

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
    mouse_pos: (f64, f64),
    screen_size: (f32, f32),
    /// Layer surface IDs: [top, bottom, left, right]
    overlay_ids: [Option<Id>; 4],
    glow_cache: GlowCache,
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
    MouseMoved(f64, f64),
    ApplyPreset(&'static str),
    Quit,
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
            settings: RingLightSettings::default(),
            camera_active: false,
            mouse_pos: (960.0, 540.0),
            screen_size: (1920.0, 1080.0),
            overlay_ids: [None; 4],
            glow_cache: GlowCache::new(),
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
        // Popup controls
        if self.popup == Some(id) {
            return self.popup_view();
        }

        // Overlay glow surface
        for (i, sid) in self.overlay_ids.iter().enumerate() {
            if *sid == Some(id) {
                return self.overlay_view(overlay::edge_from_index(i));
            }
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
                self.glow_cache.clear_all();
                return self.sync_overlay();
            }
            Message::ToggleAutoMode(on) => {
                self.settings.auto_mode = on;
                if on {
                    self.settings.enabled = self.camera_active;
                }
                self.glow_cache.clear_all();
                return self.sync_overlay();
            }

            Message::SetBrightness(b) => {
                self.settings.brightness = b;
                self.glow_cache.clear_all();
            }
            Message::SetColorTemp(t) => {
                self.settings.color_temp = t;
                self.glow_cache.clear_all();
            }
            Message::SetGlowSize(s) => {
                self.settings.glow_size = s;
                self.glow_cache.clear_all();
                // Surface sizes change → recreate
                return self.recreate_overlay();
            }
            Message::SetHoleSize(s) => {
                self.settings.hole_size = s;
                self.glow_cache.clear_all();
            }

            Message::CameraStateChanged(active) => {
                self.camera_active = active;
                if self.settings.auto_mode {
                    self.settings.enabled = active;
                    self.glow_cache.clear_all();
                    return self.sync_overlay();
                }
            }
            Message::MouseMoved(x, y) => {
                self.mouse_pos = (x, y);
                if self.settings.hole_size != HoleSize::Off && self.is_active() {
                    self.glow_cache.clear_all();
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
                self.glow_cache.clear_all();
                return self.recreate_overlay();
            }

            Message::Quit => {
                std::process::exit(0);
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

        let mouse_sub = {
            let (sw, sh) = (self.screen_size.0 as u32, self.screen_size.1 as u32);
            Subscription::run_with((sw, sh), move |&(sw, sh)| {
                async move {
                    let rx =
                        crate::mouse::start_tracker(sw as f64, sh as f64);
                    futures_util::stream::unfold(rx, |mut rx| async {
                        rx.changed().await.ok()?;
                        let (x, y) = *rx.borrow();
                        Some((Message::MouseMoved(x, y), rx))
                    })
                }
                .flatten_stream()
            })
        };

        Subscription::batch(vec![camera_sub, mouse_sub])
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

    /// Destroy then recreate (e.g. after glow-size change).
    fn recreate_overlay(&mut self) -> Task<Message> {
        let destroy = self.destroy_overlay();
        let create = if self.is_active() {
            self.create_overlay()
        } else {
            Task::none()
        };
        Task::batch(vec![destroy, create])
    }

    fn create_overlay(&mut self) -> Task<Message> {
        let glow_w = self.settings.glow_width() as u32;
        let mut cmds = Vec::new();

        let edges: [(Anchor, (Option<u32>, Option<u32>)); 4] = [
            // Top: anchor T+L+R, full width, height = glow_w
            (
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                (None, Some(glow_w)),
            ),
            // Bottom: anchor B+L+R
            (
                Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                (None, Some(glow_w)),
            ),
            // Left: anchor L+T+B, width = glow_w, full height
            (
                Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                (Some(glow_w), None),
            ),
            // Right: anchor R+T+B
            (
                Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
                (Some(glow_w), None),
            ),
        ];

        for (i, (anchor, size)) in edges.into_iter().enumerate() {
            if self.overlay_ids[i].is_some() {
                continue;
            }
            let id = Id::unique();
            self.overlay_ids[i] = Some(id);

            cmds.push(get_layer_surface(SctkLayerSurfaceSettings {
                id,
                keyboard_interactivity: KeyboardInteractivity::None,
                namespace: format!("ringlight-edge-{}", i),
                layer: Layer::Overlay,
                size: Some(size),
                anchor,
                exclusive_zone: 0,
                ..Default::default()
            }));
        }

        Task::batch(cmds)
    }

    fn destroy_overlay(&mut self) -> Task<Message> {
        let cmds: Vec<_> = self
            .overlay_ids
            .iter_mut()
            .filter_map(|slot| slot.take().map(destroy_layer_surface))
            .collect();
        Task::batch(cmds)
    }

    // -- View builders -------------------------------------------------------

    fn overlay_view(&self, edge: EdgeSide) -> Element<'_, Message> {
        let program = GlowProgram {
            cache: self.glow_cache.cache_for(edge),
            settings: &self.settings,
            mouse_pos: (self.mouse_pos.0 as f32, self.mouse_pos.1 as f32),
            screen_size: self.screen_size,
            edge,
        };
        Canvas::new(program)
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
                    .step(0.05),
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
                    .step(0.05),
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
            )
            .push(widget::divider::horizontal::default())
            .push(widget::button::text("Quit").on_press(Message::Quit));

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

