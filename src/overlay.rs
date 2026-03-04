use gtk::prelude::*;
use crate::layer_shell;
use crate::mouse;
use crate::renderer;
use crate::settings::SharedState;

pub fn create_overlay(state: SharedState) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Ringlight");
    window.set_decorated(false);
    window.set_app_paintable(true);

    // Set RGBA visual for transparency
    if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }

    // Get monitor geometry for sizing
    let (mon_w, mon_h) = get_monitor_size(&window);

    if layer_shell::is_supported() {
        eprintln!("ringlight: using layer-shell overlay");
        setup_layer_shell(&window);
    } else {
        eprintln!("ringlight: layer-shell not supported, using GNOME fallback");
        setup_gnome_fallback(&window, mon_w, mon_h);
    }

    // Create drawing area
    let drawing_area = gtk::DrawingArea::new();
    window.add(&drawing_area);

    // Set up drawing
    let state_draw = state.clone();
    drawing_area.connect_draw(move |widget, cr| {
        let alloc = widget.allocation();
        let w = alloc.width() as f64;
        let h = alloc.height() as f64;

        let s = state_draw.lock().unwrap().clone();
        renderer::draw_glow(cr, w, h, &s);

        glib::Propagation::Stop
    });

    // Make click-through by setting empty input region
    window.connect_realize(|win| {
        set_click_through(win);
    });

    window.show_all();

    // Re-apply input shape after show
    set_click_through(&window);

    // Start raw mouse tracker (reads /dev/input/eventN directly — works on Wayland)
    if let Some(mouse_pos) = mouse::start_mouse_tracker(mon_w as f64, mon_h as f64) {
        let state_mouse = state.clone();
        let window_mouse = window.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let (mx, my) = *mouse_pos.lock().unwrap();
            let mut s = state_mouse.lock().unwrap();
            if (s.mouse_x - mx).abs() > 2.0 || (s.mouse_y - my).abs() > 2.0 {
                s.mouse_x = mx;
                s.mouse_y = my;
                drop(s);
                window_mouse.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    } else {
        eprintln!("ringlight: mouse tracking unavailable, dimming disabled");
    }

    window
}

fn get_monitor_size(window: &gtk::Window) -> (i32, i32) {
    if let Some(screen) = gtk::prelude::WidgetExt::screen(window) {
        let display = screen.display();
        if let Some(monitor) = display.primary_monitor().or_else(|| display.monitor(0)) {
            let geom = monitor.geometry();
            return (geom.width(), geom.height());
        }
    }
    (1920, 1080)
}

fn setup_layer_shell(window: &gtk::Window) {
    layer_shell::init_for_window(window);
    layer_shell::set_layer(window, layer_shell::Layer::Overlay);
    layer_shell::set_anchor(window, layer_shell::Edge::Top, true);
    layer_shell::set_anchor(window, layer_shell::Edge::Bottom, true);
    layer_shell::set_anchor(window, layer_shell::Edge::Left, true);
    layer_shell::set_anchor(window, layer_shell::Edge::Right, true);
    layer_shell::set_exclusive_zone(window, -1);
    layer_shell::set_keyboard_mode(window, layer_shell::KeyboardMode::None);
    layer_shell::set_namespace(window, "ringlight");
}

fn setup_gnome_fallback(window: &gtk::Window, mon_w: i32, mon_h: i32) {
    window.set_default_size(mon_w, mon_h);
    window.move_(0, 0);

    // Stay above all windows, skip taskbar/pager
    window.set_keep_above(true);
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);
    window.set_accept_focus(false);
    window.set_focus_on_map(false);
    window.set_type_hint(gdk::WindowTypeHint::Dock);
    window.stick(); // visible on all workspaces
}

fn set_click_through(window: &gtk::Window) {
    if let Some(gdk_win) = window.window() {
        let empty_region = cairo::Region::create();
        gdk_win.input_shape_combine_region(&empty_region, 0, 0);
    }
}

/// Request a redraw of the overlay
pub fn queue_redraw(window: &gtk::Window) {
    window.queue_draw();
}
