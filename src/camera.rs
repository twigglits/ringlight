use crate::tray::TrayCommand;
use std::fs;
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

/// Check if any process has a video device open by scanning /proc/*/fd/ symlinks.
fn is_camera_in_use(devices: &[String]) -> bool {
    if devices.is_empty() {
        return false;
    }

    let proc_entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    for entry in proc_entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Only look at numeric (PID) directories
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let fd_dir = entry.path().join("fd");
        let fd_entries = match fs::read_dir(&fd_dir) {
            Ok(entries) => entries,
            Err(_) => continue, // permission denied or process exited
        };

        for fd_entry in fd_entries.flatten() {
            let link_target = match fs::read_link(fd_entry.path()) {
                Ok(target) => target,
                Err(_) => continue,
            };
            let target_str = link_target.to_string_lossy().into_owned();
            if devices.iter().any(|dev| target_str == *dev) {
                return true;
            }
        }
    }

    false
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
