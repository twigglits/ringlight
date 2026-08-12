use std::fs::File;
use std::io::Read;
use std::mem;
use std::sync::{Arc, Mutex};

pub type SharedMousePos = Arc<Mutex<(f64, f64)>>;

// --- D-Bus cursor tracking (via GNOME Shell extension) ---

/// Try to get cursor position from the ringlight GNOME Shell extension.
/// Returns None if the extension is not installed/enabled.
pub fn dbus_cursor_position() -> Option<(f64, f64)> {
    let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let result = conn.call_sync(
        Some("com.github.ringlight.Cursor"),
        "/com/github/ringlight/Cursor",
        "com.github.ringlight.Cursor",
        "GetPosition",
        None,
        Some(&glib::VariantType::new("(ii)").unwrap()),
        gio::DBusCallFlags::NONE,
        100, // 100ms timeout
        gio::Cancellable::NONE,
    ).ok()?;
    let x: i32 = result.child_value(0).get()?;
    let y: i32 = result.child_value(1).get()?;
    Some((x as f64, y as f64))
}

// --- Raw input event tracking (fallback) ---

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

fn find_mouse_event_device() -> Option<String> {
    let devices = std::fs::read_to_string("/proc/bus/input/devices").ok()?;
    for section in devices.split("\n\n") {
        let Some(handlers_line) = section.lines().find(|l| l.starts_with("H: Handlers=")) else {
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
pub fn start_raw_tracker(screen_width: f64, screen_height: f64) -> Option<SharedMousePos> {
    let device = find_mouse_event_device().or_else(|| {
        eprintln!("ringlight: could not find mouse event device in /proc/bus/input/devices");
        None
    })?;

    let file = match File::open(&device) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ringlight: cannot open {} for mouse tracking: {}", device, e);
            return None;
        }
    };

    eprintln!("ringlight: raw mouse tracking via {}", device);

    let pos: SharedMousePos = Arc::new(Mutex::new((screen_width / 2.0, screen_height / 2.0)));
    let pos_thread = pos.clone();

    std::thread::spawn(move || {
        let mut file = file;
        let event_size = mem::size_of::<InputEvent>();
        let mut buf = vec![0u8; event_size];

        loop {
            if file.read_exact(&mut buf).is_err() {
                break;
            }
            let event: &InputEvent = unsafe { &*(buf.as_ptr() as *const InputEvent) };
            if event.type_ == EV_REL {
                let Ok(mut p) = pos_thread.lock() else { break };
                match event.code {
                    REL_X => p.0 = (p.0 + event.value as f64).clamp(0.0, screen_width),
                    REL_Y => p.1 = (p.1 + event.value as f64).clamp(0.0, screen_height),
                    _ => {}
                }
            }
        }
    });

    Some(pos)
}
