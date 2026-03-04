use std::fs::File;
use std::io::Read;
use std::mem;
use std::os::raw::{c_int, c_uint, c_ulong};
use std::sync::{Arc, Mutex};

// --- Raw input event tracking ---

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

fn find_mouse_event_device() -> Option<String> {
    let devices = std::fs::read_to_string("/proc/bus/input/devices").ok()?;
    for section in devices.split("\n\n") {
        let handlers_line = section.lines().find(|l| l.starts_with("H: Handlers="))?;
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

// --- X11 pointer query (for initial position via XWayland) ---

type XDisplay = *mut std::ffi::c_void;
type XWindow = c_ulong;

#[link(name = "X11")]
extern "C" {
    fn XOpenDisplay(display_name: *const i8) -> XDisplay;
    fn XCloseDisplay(display: XDisplay) -> c_int;
    fn XDefaultRootWindow(display: XDisplay) -> XWindow;
    fn XQueryPointer(
        display: XDisplay,
        w: XWindow,
        root_return: *mut XWindow,
        child_return: *mut XWindow,
        root_x_return: *mut c_int,
        root_y_return: *mut c_int,
        win_x_return: *mut c_int,
        win_y_return: *mut c_int,
        mask_return: *mut c_uint,
    ) -> c_int;
}

/// Query the current cursor position via X11/XWayland (one-shot).
fn x11_cursor_position() -> Option<(f64, f64)> {
    unsafe {
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return None;
        }
        let root = XDefaultRootWindow(display);
        let (mut root_ret, mut child_ret) = (0 as XWindow, 0 as XWindow);
        let (mut rx, mut ry, mut wx, mut wy) = (0 as c_int, 0, 0, 0);
        let mut mask: c_uint = 0;
        XQueryPointer(
            display, root,
            &mut root_ret, &mut child_ret,
            &mut rx, &mut ry, &mut wx, &mut wy, &mut mask,
        );
        XCloseDisplay(display);
        Some((rx as f64, ry as f64))
    }
}

/// Start a background thread that reads raw mouse events from /dev/input/eventN.
/// Uses X11/XWayland for initial position, then tracks relative deltas.
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

    // Try to get initial cursor position from X11/XWayland
    let initial = x11_cursor_position().unwrap_or((screen_width / 2.0, screen_height / 2.0));
    eprintln!("ringlight: tracking mouse via {} (initial pos: {:.0},{:.0})", device, initial.0, initial.1);

    let pos: SharedMousePos = Arc::new(Mutex::new(initial));
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
