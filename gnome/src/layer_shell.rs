/// Layer shell layers
#[repr(u32)]
#[allow(dead_code)]
pub enum Layer {
    Background = 0,
    Bottom = 1,
    Top = 2,
    Overlay = 3,
}

/// Layer shell edges
#[repr(u32)]
#[allow(dead_code)]
pub enum Edge {
    Top = 0,
    Bottom = 1,
    Left = 2,
    Right = 3,
}

/// Keyboard interactivity modes
#[repr(u32)]
#[allow(dead_code)]
pub enum KeyboardMode {
    None = 0,
    Exclusive = 1,
    OnDemand = 2,
}

// FFI declarations for libgtk-layer-shell
extern "C" {
    fn gtk_layer_is_supported() -> i32;
    fn gtk_layer_init_for_window(window: *mut std::ffi::c_void);
    fn gtk_layer_set_layer(window: *mut std::ffi::c_void, layer: u32);
    fn gtk_layer_set_anchor(window: *mut std::ffi::c_void, edge: u32, anchor: i32);
    fn gtk_layer_set_exclusive_zone(window: *mut std::ffi::c_void, zone: i32);
    fn gtk_layer_set_keyboard_mode(window: *mut std::ffi::c_void, mode: u32);
    fn gtk_layer_set_namespace(window: *mut std::ffi::c_void, namespace: *const std::ffi::c_char);
}

/// Check if the compositor supports layer shell
pub fn is_supported() -> bool {
    unsafe { gtk_layer_is_supported() != 0 }
}

/// Get the raw GObject pointer from a GTK window
fn window_ptr(window: &gtk::Window) -> *mut std::ffi::c_void {
    use glib::translate::ToGlibPtr;
    let ptr: *mut gtk::ffi::GtkWindow = window.to_glib_none().0;
    ptr as *mut std::ffi::c_void
}

/// Initialize layer shell for a window
pub fn init_for_window(window: &gtk::Window) {
    unsafe {
        gtk_layer_init_for_window(window_ptr(window));
    }
}

/// Set the layer for a window
pub fn set_layer(window: &gtk::Window, layer: Layer) {
    unsafe {
        gtk_layer_set_layer(window_ptr(window), layer as u32);
    }
}

/// Set anchor for a specific edge
pub fn set_anchor(window: &gtk::Window, edge: Edge, anchor: bool) {
    unsafe {
        gtk_layer_set_anchor(window_ptr(window), edge as u32, anchor as i32);
    }
}

/// Set the exclusive zone (-1 = don't push other windows)
pub fn set_exclusive_zone(window: &gtk::Window, zone: i32) {
    unsafe {
        gtk_layer_set_exclusive_zone(window_ptr(window), zone);
    }
}

/// Set keyboard interactivity mode
pub fn set_keyboard_mode(window: &gtk::Window, mode: KeyboardMode) {
    unsafe {
        gtk_layer_set_keyboard_mode(window_ptr(window), mode as u32);
    }
}

/// Set the namespace for the layer surface
pub fn set_namespace(window: &gtk::Window, namespace: &str) {
    let c_namespace = std::ffi::CString::new(namespace).expect("Invalid namespace string");
    unsafe {
        gtk_layer_set_namespace(window_ptr(window), c_namespace.as_ptr());
    }
}
