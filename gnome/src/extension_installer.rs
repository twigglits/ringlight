use std::fs;
use std::path::PathBuf;

const EXTENSION_JS: &str = include_str!("../gnome-extension/extension.js");
const METADATA_JSON: &str = include_str!("../gnome-extension/metadata.json");

const EXTENSION_UUID: &str = "ringlight-cursor@ringlight";

/// Install or update the GNOME Shell extension if missing or outdated.
/// Called early in startup so the D-Bus cursor interface is available.
pub fn ensure_extension_installed() {
    let Some(data_dir) = dirs_path() else {
        eprintln!("ringlight: could not determine ~/.local/share, skipping extension install");
        return;
    };

    let ext_dir = data_dir.join("gnome-shell/extensions").join(EXTENSION_UUID);

    let needs_install = if ext_dir.join("extension.js").exists() {
        // Check if bundled version differs from installed
        let installed_js = fs::read_to_string(ext_dir.join("extension.js")).unwrap_or_default();
        let installed_meta = fs::read_to_string(ext_dir.join("metadata.json")).unwrap_or_default();
        installed_js != EXTENSION_JS || installed_meta != METADATA_JSON
    } else {
        true
    };

    if !needs_install {
        return;
    }

    if let Err(e) = fs::create_dir_all(&ext_dir) {
        eprintln!("ringlight: failed to create extension directory: {}", e);
        return;
    }

    let mut ok = true;
    if let Err(e) = fs::write(ext_dir.join("extension.js"), EXTENSION_JS) {
        eprintln!("ringlight: failed to write extension.js: {}", e);
        ok = false;
    }
    if let Err(e) = fs::write(ext_dir.join("metadata.json"), METADATA_JSON) {
        eprintln!("ringlight: failed to write metadata.json: {}", e);
        ok = false;
    }

    if ok {
        eprintln!("ringlight: installed GNOME Shell extension '{}'", EXTENSION_UUID);
        eprintln!("ringlight: enable it with: gnome-extensions enable {}", EXTENSION_UUID);
        eprintln!("ringlight: then restart GNOME Shell (log out/in, or Alt+F2 → r on X11)");
    }
}

fn dirs_path() -> Option<PathBuf> {
    // $XDG_DATA_HOME or ~/.local/share
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".local/share"))
}
