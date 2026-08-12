fn main() {
    pkg_config::Config::new()
        .atleast_version("0.6")
        .probe("gtk-layer-shell-0")
        .expect("gtk-layer-shell-0 not found. Install with: sudo apt install libgtk-layer-shell-dev");

    pkg_config::Config::new()
        .probe("x11")
        .expect("libX11 not found. Install with: sudo apt install libx11-dev");
}
