use crate::tray::TrayCommand;
use std::process::Command;
use std::time::Duration;

/// Detect video devices present on the system
fn detect_video_devices() -> Vec<String> {
    let mut devices = Vec::new();
    for i in 0..10 {
        let path = format!("/dev/video{}", i);
        if std::path::Path::new(&path).exists() {
            devices.push(path);
        }
    }
    devices
}

/// Check if any process has a video device open using `fuser`
fn is_camera_in_use(devices: &[String]) -> bool {
    if devices.is_empty() {
        return false;
    }

    let result = Command::new("fuser")
        .args(devices)
        .stderr(std::process::Stdio::null())
        .output();

    match result {
        Ok(output) => {
            // fuser returns exit code 0 if any file is accessed by a process
            output.status.success()
        }
        Err(_) => false,
    }
}

/// Start a background thread that polls camera usage every 2 seconds.
/// Sends `TrayCommand::CameraStateChanged(bool)` on the glib channel when state changes.
pub fn start_camera_monitor(sender: glib::Sender<TrayCommand>) {
    std::thread::spawn(move || {
        let devices = detect_video_devices();
        if devices.is_empty() {
            eprintln!("ringlight: no /dev/video* devices found, camera monitor disabled");
            return;
        }
        eprintln!("ringlight: monitoring camera devices: {:?}", devices);

        let mut was_active = false;

        loop {
            let active = is_camera_in_use(&devices);

            if active != was_active {
                eprintln!("ringlight: camera {} active", if active { "became" } else { "no longer" });
                if sender.send(TrayCommand::CameraStateChanged(active)).is_err() {
                    break; // receiver dropped, app shutting down
                }
                was_active = active;
            }

            std::thread::sleep(Duration::from_secs(2));
        }
    });
}
