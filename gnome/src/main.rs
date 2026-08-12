mod camera;
mod extension_installer;
mod layer_shell;
mod mouse;
mod overlay;
mod renderer;
mod settings;
mod tray;

use gtk::prelude::*;
use settings::{new_shared_state, GlowSize, Preset};
use tray::TrayCommand;

fn main() {
    let application = gtk::Application::new(
        Some("com.github.ringlight"),
        gio::ApplicationFlags::FLAGS_NONE,
    );

    application.connect_activate(|app| {
        extension_installer::ensure_extension_installed();

        let state = new_shared_state();

        // Create overlay window
        let window = overlay::create_overlay(state.clone());
        app.add_window(&window);

        // Start system tray and get channel for commands
        let (receiver, camera_sender) = tray::start_tray(state.clone());

        // Start camera monitor (sends CameraStateChanged via the same channel)
        camera::start_camera_monitor(camera_sender);

        let state_cmd = state.clone();
        let window_cmd = window.clone();
        receiver.attach(None, move |cmd| {
            {
                let mut s = state_cmd.lock().unwrap();
                match cmd {
                    TrayCommand::Toggle => {
                        s.enabled = !s.enabled;
                    }
                    TrayCommand::ToggleAutoMode => {
                        s.auto_mode = !s.auto_mode;
                        // If turning auto on, sync with current camera state
                        if s.auto_mode {
                            s.enabled = s.camera_active;
                        }
                    }
                    TrayCommand::BrightnessUp => {
                        s.brightness = (s.brightness + 0.1).min(1.0);
                    }
                    TrayCommand::BrightnessDown => {
                        s.brightness = (s.brightness - 0.1).max(0.0);
                    }
                    TrayCommand::Warmer => {
                        s.color_temp = (s.color_temp - 0.1).max(0.0);
                    }
                    TrayCommand::Cooler => {
                        s.color_temp = (s.color_temp + 0.1).min(1.0);
                    }
                    TrayCommand::SetGlowSize(size) => {
                        s.glow_size = size;
                    }
                    TrayCommand::SetHoleSize(size) => {
                        s.hole_size = size;
                    }
                    TrayCommand::ApplyPreset(p) => {
                        match p {
                            Preset::WarmReading => {
                                s.brightness = 0.5;
                                s.color_temp = 0.1;
                                s.glow_size = GlowSize::Small;
                            }
                            Preset::CoolDaylight => {
                                s.brightness = 0.8;
                                s.color_temp = 0.9;
                                s.glow_size = GlowSize::Medium;
                            }
                            Preset::Subtle => {
                                s.brightness = 0.3;
                                s.color_temp = 0.5;
                                s.glow_size = GlowSize::Small;
                            }
                            Preset::Bright => {
                                s.brightness = 1.0;
                                s.color_temp = 0.5;
                                s.glow_size = GlowSize::Large;
                            }
                        }
                    }
                    TrayCommand::CameraStateChanged(active) => {
                        s.camera_active = active;
                        if s.auto_mode {
                            s.enabled = active;
                        }
                    }
                    TrayCommand::Quit => {
                        gtk::main_quit();
                        return glib::ControlFlow::Break;
                    }
                }
            }
            overlay::queue_redraw(&window_cmd);
            glib::ControlFlow::Continue
        });
    });

    application.run();
}
