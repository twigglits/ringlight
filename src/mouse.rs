use std::fs::File;
use std::io::Read;
use std::mem;
use std::sync::{Arc, Mutex};

/// Linux input_event struct (from linux/input.h)
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

pub type SharedMousePos = Arc<Mutex<(f64, f64)>>;

/// Find the mouse event device by checking /proc/bus/input/devices
fn find_mouse_event_device() -> Option<String> {
    let devices = std::fs::read_to_string("/proc/bus/input/devices").ok()?;
    // Look for a section containing "mouse" in handlers line
    for section in devices.split("\n\n") {
        let handlers_line = section.lines().find(|l| l.starts_with("H: Handlers="))?;
        if handlers_line.contains("mouse") {
            // Extract eventN from the handlers line
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
/// Returns a shared mouse position that gets updated with each movement.
pub fn start_mouse_tracker(screen_width: f64, screen_height: f64) -> Option<SharedMousePos> {
    let device = find_mouse_event_device().or_else(|| {
        eprintln!("ringlight: could not find mouse event device, trying /dev/input/event3");
        Some("/dev/input/event3".to_string())
    })?;

    let file = match File::open(&device) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("ringlight: cannot open {} for mouse tracking: {}", device, e);
            return None;
        }
    };

    eprintln!("ringlight: tracking mouse via {} (screen {}x{})", device, screen_width, screen_height);

    // Start at center of screen
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
                let mut p = pos_thread.lock().unwrap();
                match event.code {
                    REL_X => {
                        p.0 = (p.0 + event.value as f64).clamp(0.0, screen_width);
                    }
                    REL_Y => {
                        p.1 = (p.1 + event.value as f64).clamp(0.0, screen_height);
                    }
                    _ => {}
                }
            }
        }
    });

    Some(pos)
}
