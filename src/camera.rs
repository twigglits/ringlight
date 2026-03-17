use std::fs;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;

/// Detect video devices present on the system.
fn detect_video_devices() -> Vec<String> {
    (0..10)
        .map(|i| format!("/dev/video{}", i))
        .filter(|p| std::path::Path::new(p).exists())
        .collect()
}

/// Check if any process has a video device open by scanning /proc/*/fd/ symlinks.
fn is_camera_in_use(devices: &[String]) -> bool {
    if devices.is_empty() {
        return false;
    }

    let Ok(proc_entries) = fs::read_dir("/proc") else {
        return false;
    };

    for entry in proc_entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let fd_dir = entry.path().join("fd");
        let Ok(fds) = fs::read_dir(&fd_dir) else {
            continue;
        };

        for fd in fds.flatten() {
            if let Ok(target) = fs::read_link(fd.path()) {
                let target_str = target.to_string_lossy();
                if devices.iter().any(|dev| *dev == *target_str) {
                    return true;
                }
            }
        }
    }

    false
}

/// Async camera monitor that sends `true`/`false` on state changes.
/// Polls every 2 seconds.
pub async fn monitor_camera(tx: mpsc::Sender<bool>) {
    let devices = detect_video_devices();
    if devices.is_empty() {
        log::warn!("No /dev/video* devices found, camera monitor disabled");
        // Keep the future alive so the subscription doesn't restart
        std::future::pending::<()>().await;
        return;
    }
    log::info!("Monitoring camera devices: {:?}", devices);

    let mut was_active = false;
    let mut interval = time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;
        let active = is_camera_in_use(&devices);
        if active != was_active {
            log::info!(
                "Camera {} active",
                if active { "became" } else { "no longer" }
            );
            if tx.send(active).await.is_err() {
                break;
            }
            was_active = active;
        }
    }
}
