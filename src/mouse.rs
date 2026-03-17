use std::fs;
use std::io::Read;
use std::mem;
use tokio::sync::watch;

/// Raw Linux input event (from linux/input.h).
#[repr(C)]
struct InputEvent {
    _tv_sec: isize,
    _tv_usec: isize,
    type_: u16,
    code: u16,
    value: i32,
}

const EV_REL: u16 = 0x02;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;

/// Find a mouse event device by parsing /proc/bus/input/devices.
fn find_mouse_event_device() -> Option<String> {
    let devices = fs::read_to_string("/proc/bus/input/devices").ok()?;
    for section in devices.split("\n\n") {
        let Some(handlers_line) = section.lines().find(|l| l.starts_with("H: Handlers="))
        else {
            continue;
        };
        if handlers_line.contains("mouse") {
            for part in handlers_line.split_whitespace() {
                if part.starts_with("event") {
                    return Some(format!("/dev/input/{}", part));
                }
            }
        }
    }
    None
}

/// Start a background thread that reads raw mouse events from /dev/input/eventN.
/// Returns a watch receiver that always holds the latest (x, y) screen position.
///
/// On COSMIC/Wayland there is no GNOME Shell extension for absolute cursor
/// coordinates, so we accumulate relative motion events and clamp to screen bounds.
pub fn start_tracker(screen_width: f64, screen_height: f64) -> watch::Receiver<(f64, f64)> {
    let (tx, rx) = watch::channel((screen_width / 2.0, screen_height / 2.0));

    std::thread::spawn(move || {
        let Some(device) = find_mouse_event_device() else {
            log::warn!("Could not find mouse event device in /proc/bus/input/devices");
            return;
        };

        let mut file = match fs::File::open(&device) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Cannot open {} for mouse tracking: {}", device, e);
                return;
            }
        };

        log::info!("Raw mouse tracking via {}", device);

        let event_size = mem::size_of::<InputEvent>();
        let mut buf = vec![0u8; event_size];
        let mut x = screen_width / 2.0;
        let mut y = screen_height / 2.0;

        loop {
            if file.read_exact(&mut buf).is_err() {
                break;
            }
            let event: &InputEvent = unsafe { &*(buf.as_ptr() as *const InputEvent) };
            if event.type_ == EV_REL {
                match event.code {
                    REL_X => x = (x + event.value as f64).clamp(0.0, screen_width),
                    REL_Y => y = (y + event.value as f64).clamp(0.0, screen_height),
                    _ => {}
                }
                let _ = tx.send((x, y));
            }
        }
    });

    rx
}
