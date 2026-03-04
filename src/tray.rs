use crate::settings::SharedState;

/// Messages from tray to main GTK loop
#[derive(Debug, Clone)]
pub enum TrayCommand {
    Toggle,
    ToggleAutoMode,
    BrightnessUp,
    BrightnessDown,
    Warmer,
    Cooler,
    CameraStateChanged(bool),
    Quit,
}

struct RingLightTray {
    state: SharedState,
    sender: glib::Sender<TrayCommand>,
}

impl ksni::Tray for RingLightTray {
    fn id(&self) -> String {
        "ringlight".to_string()
    }

    fn title(&self) -> String {
        "Ring Light".to_string()
    }

    fn icon_name(&self) -> String {
        "display-brightness-symbolic".to_string()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let state = self.state.lock().unwrap();
        let status = if state.enabled { "ON" } else { "OFF" };
        let camera = if state.camera_active { "Camera: active" } else { "Camera: off" };
        ksni::ToolTip {
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
            title: format!("Ring Light ({})", status),
            description: format!(
                "Brightness: {:.0}% | Color: {} | {}",
                state.brightness * 100.0,
                if state.color_temp < 0.4 {
                    "Warm"
                } else if state.color_temp > 0.6 {
                    "Cool"
                } else {
                    "Neutral"
                },
                camera,
            ),
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;

        let state = self.state.lock().unwrap();
        let enabled = state.enabled;
        let auto_mode = state.auto_mode;
        drop(state);

        vec![
            StandardItem {
                label: if enabled {
                    "Turn Off".to_string()
                } else {
                    "Turn On".to_string()
                },
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::Toggle);
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Auto (Camera)".to_string(),
                checked: auto_mode,
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::ToggleAutoMode);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Brightness Up".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::BrightnessUp);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Brightness Down".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::BrightnessDown);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Warmer".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::Warmer);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Cooler".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::Cooler);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.sender.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Start the system tray on a background thread.
/// Returns a glib Receiver for tray commands, and a Sender for the camera monitor.
pub fn start_tray(state: SharedState) -> (glib::Receiver<TrayCommand>, glib::Sender<TrayCommand>) {
    #[allow(deprecated)]
    let (sender, receiver) = glib::MainContext::channel(glib::Priority::DEFAULT);

    let tray_sender = sender.clone();
    std::thread::spawn(move || {
        let service = ksni::TrayService::new(RingLightTray { state, sender: tray_sender });
        match service.run() {
            Ok(()) => eprintln!("ringlight: tray service exited"),
            Err(e) => eprintln!("ringlight: tray service error: {}", e),
        }
    });

    (receiver, sender)
}
