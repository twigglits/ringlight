fn main() {
    pkg_config::Config::new()
        .atleast_version("0.6")
        .probe("gtk-layer-shell-0")
        .expect("gtk-layer-shell-0 not found. Install with: sudo apt install libgtk-layer-shell-dev");
}
