//! Applet → overlay-process channel.
//!
//! The overlay cannot live in the applet process. cosmic-panel spawns applets
//! with `WAYLAND_SOCKET` set to a pre-connected fd, and libwayland prefers that
//! fd over `WAYLAND_DISPLAY`; a layer surface created on it does not get a
//! working empty input region, so the full-screen glow swallows every click.
//! See CLAUDE.md ("Why the overlay is a separate process").
//!
//! So the applet spawns a child with that variable stripped, which connects to
//! the session compositor directly, and streams settings to it as one JSON
//! object per line on stdin. Reusing stdin is deliberate: when the applet dies
//! the pipe closes, the child reads EOF and exits, so a full-screen overlay can
//! never be stranded on screen by `pkill`.

use crate::settings::RingLightSettings;
use std::process::{Child, ChildStdin, Command};

/// The variable cosmic-panel uses to hand an applet its own connection.
const PANEL_SOCKET_VAR: &str = "WAYLAND_SOCKET";

/// Environment for the overlay child: the applet's own, minus the panel socket.
pub fn sanitized_env(
    vars: impl IntoIterator<Item = (String, String)>,
) -> Vec<(String, String)> {
    vars.into_iter()
        .filter(|(k, _)| k != PANEL_SOCKET_VAR)
        .collect()
}

/// Encode settings as one line, ready to write to the child's stdin.
pub fn encode(settings: &RingLightSettings) -> String {
    // serde_json never emits a bare newline inside a compact object, so one
    // object really is one line.
    match serde_json::to_string(settings) {
        Ok(json) => format!("{json}\n"),
        Err(e) => {
            log::warn!("ringlight: could not encode settings: {e}");
            String::new()
        }
    }
}

/// Decode one line from the applet. `None` for anything unparseable, so a
/// garbled line is skipped rather than taking the overlay down.
pub fn decode(line: &str) -> Option<RingLightSettings> {
    serde_json::from_str(line).ok()
}

/// Flag that puts the binary into overlay mode.
pub const OVERLAY_FLAG: &str = "--overlay";

/// Build the command that launches the overlay child.
///
/// `env` is the applet's environment; the panel socket is removed from it so
/// the child connects to the session compositor instead of cosmic-panel.
pub fn overlay_command_with_env(
    exe: &std::path::Path,
    env: impl IntoIterator<Item = (String, String)>,
) -> Command {
    let mut command = Command::new(exe);
    command.arg(OVERLAY_FLAG);
    // Clear first: overriding is not enough, an unset variable is still
    // inherited, and inheriting this one is the entire bug.
    command.env_clear();
    command.envs(sanitized_env(env));
    command
}

/// As [`overlay_command_with_env`], using this process's own environment.
pub fn overlay_command(exe: &std::path::Path) -> Command {
    overlay_command_with_env(exe, std::env::vars())
}

/// A running overlay child. Dropping it kills the child, so the glow cannot
/// outlive the applet that owns it.
pub struct OverlayProcess {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl OverlayProcess {
    pub fn spawn(mut command: Command) -> std::io::Result<Self> {
        let mut child = command.stdin(std::process::Stdio::piped()).spawn()?;
        let stdin = child.stdin.take();
        Ok(Self { child, stdin })
    }

    /// Push settings to the child. Returns false once the pipe is broken.
    pub fn send(&mut self, settings: &RingLightSettings) -> bool {
        use std::io::Write;
        let Some(stdin) = self.stdin.as_mut() else {
            return false;
        };
        let line = encode(settings);
        if stdin.write_all(line.as_bytes()).is_err() || stdin.flush().is_err() {
            self.stdin = None;
            return false;
        }
        true
    }
}

impl Drop for OverlayProcess {
    fn drop(&mut self) {
        // Closing stdin alone would be enough for a healthy child, but a wedged
        // one must not be left holding a full-screen surface.
        self.stdin = None;
        let _ = self.child.kill();
        // Reap it, so the pid is really gone and not left as a zombie.
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{GlowSize, HoleSize};

    fn env() -> Vec<(String, String)> {
        vec![
            ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
            ("WAYLAND_SOCKET".into(), "62".into()),
            ("XDG_RUNTIME_DIR".into(), "/run/user/1000".into()),
        ]
    }

    #[test]
    fn sanitized_env_drops_the_panel_socket() {
        let out = sanitized_env(env());
        assert!(
            !out.iter().any(|(k, _)| k == "WAYLAND_SOCKET"),
            "WAYLAND_SOCKET must not reach the child; it is the whole bug"
        );
    }

    #[test]
    fn sanitized_env_keeps_the_session_display() {
        let out = sanitized_env(env());
        // Without this the child has nothing to connect to at all.
        assert!(out
            .iter()
            .any(|(k, v)| k == "WAYLAND_DISPLAY" && v == "wayland-1"));
        assert!(out.iter().any(|(k, _)| k == "XDG_RUNTIME_DIR"));
    }

    #[test]
    fn settings_survive_an_ipc_round_trip() {
        let original = RingLightSettings {
            enabled: true,
            brightness: 0.42,
            color_temp: 0.9,
            auto_mode: false,
            glow_size: GlowSize::Large,
            hole_size: HoleSize::Off,
        };

        let decoded = decode(encode(&original).trim()).expect("decodes");

        assert_eq!(decoded, original);
    }

    #[test]
    fn encoded_settings_occupy_exactly_one_line() {
        // The child reads line by line, so an embedded newline would desync it.
        let encoded = encode(&RingLightSettings::default());
        assert!(encoded.ends_with('\n'));
        assert_eq!(encoded.trim_end().lines().count(), 1);
    }

    #[test]
    fn decode_rejects_garbage_instead_of_panicking() {
        assert!(decode("not json").is_none());
        assert!(decode("").is_none());
    }

    fn cmd() -> Command {
        overlay_command_with_env(std::path::Path::new("/usr/local/bin/ringlight"), env())
    }

    #[test]
    fn overlay_command_asks_for_overlay_mode() {
        let c = cmd();
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        assert_eq!(args, vec![OVERLAY_FLAG]);
    }

    #[test]
    fn overlay_command_does_not_hand_the_child_the_panel_socket() {
        // The child must not inherit it either, so the env has to be cleared
        // and rebuilt rather than merely overridden.
        let c = cmd();
        let leaked = c
            .get_envs()
            .any(|(k, v)| k == "WAYLAND_SOCKET" && v.is_some());
        assert!(!leaked, "child would still connect through cosmic-panel");
    }

    #[test]
    fn overlay_command_forwards_the_session_display() {
        let c = cmd();
        assert!(c
            .get_envs()
            .any(|(k, v)| k == "WAYLAND_DISPLAY"
                && v.map(|v| v == "wayland-1").unwrap_or(false)));
    }

    #[test]
    fn dropping_the_handle_kills_the_child() {
        // `cat` sits reading stdin forever, exactly like the real overlay does.
        let handle = OverlayProcess::spawn(Command::new("cat")).expect("spawn cat");
        let pid = handle.child.id();
        assert!(
            std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "child should be running"
        );

        drop(handle);

        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "a stranded overlay would cover the screen with no way to dismiss it"
        );
    }
}
