mod app;
mod camera;
mod config;
mod cursor;
mod glow;
mod ipc;
mod overlay;
mod settings;

/// The binary is both the panel applet and the overlay process it spawns.
/// One binary keeps them in lockstep: the applet launches its own executable,
/// so the two halves can never be different versions.
fn wants_overlay(args: &[String]) -> bool {
    args.iter().any(|a| a == ipc::OVERLAY_FLAG)
}

fn main() -> cosmic::iced::Result {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if wants_overlay(&args) {
        return overlay::run();
    }
    cosmic::applet::run::<app::RingLight>(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_flag_selects_overlay_mode() {
        assert!(wants_overlay(&[crate::ipc::OVERLAY_FLAG.to_string()]));
    }

    #[test]
    fn no_arguments_means_panel_applet() {
        // cosmic-panel launches the applet with no arguments at all.
        assert!(!wants_overlay(&[]));
    }

    #[test]
    fn unrelated_arguments_do_not_select_overlay_mode() {
        assert!(!wants_overlay(&["--help".to_string()]));
    }
}
