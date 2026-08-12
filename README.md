# Ringlight

A desktop ring light overlay. Adds a soft, warm glow around the edges of your screen to simulate a ring light during video calls on Google Meet, Teams, Zoom, etc. Automatically activates when your camera turns on.

Ringlight ships as **two packages**, because the panel integration and the overlay surface have to be written against the desktop you are actually running. Pick the one matching yours — they conflict with each other, and both give you a `ringlight` command.

| Your desktop | Package | Panel integration |
|---|---|---|
| GNOME on Debian/Ubuntu | `ringlight-gnome.deb` | system tray (AppIndicator) |
| COSMIC on Pop!\_OS | `ringlight-cosmic.deb` | cosmic-panel applet |

## Install

Grab the `.deb` for your desktop from the [latest release](https://github.com/twigglits/ringlight/releases/latest).

### Debian/Ubuntu (GNOME desktop)

```bash
sudo apt install ./ringlight-gnome.deb
ringlight
```

Ringlight appears in the system tray. On first launch it drops a GNOME Shell extension into `~/.local/share/gnome-shell/extensions/` for accurate cursor tracking; enable it once:

```bash
gnome-extensions enable ringlight-cursor@ringlight
```

Then log out and back in. Without the extension Ringlight still runs — it falls back to reading `/dev/input`, which needs `input` group membership and only gives relative motion.

### Pop!\_OS (COSMIC desktop)

```bash
sudo apt install ./ringlight-cosmic.deb
```

Then add "Ringlight" to your panel via **Settings → Desktop → Panel → Applets**. No extension and no permissions are needed; cursor position comes from the compositor.

## Build from source

Both builds need a Rust toolchain (`rustup` / `cargo`). `cargo deb` produces the same package the release workflow does; plain `cargo build --release` gives you just the binary.

```bash
git clone https://github.com/twigglits/ringlight.git
cd ringlight
```

**COSMIC** (repository root):

```bash
sudo apt install build-essential cmake libexpat1-dev libfontconfig-dev \
  libfreetype-dev libxkbcommon-dev libwayland-dev libdbus-1-dev \
  libssl-dev libgbm-dev libpipewire-0.3-dev libpulse-dev pkgconf
cargo deb                      # or: cargo build --release
```

**GNOME** (the `gnome/` subdirectory, a separate crate with its own lockfile):

```bash
sudo apt install build-essential libgtk-3-dev libgtk-layer-shell-dev libx11-dev pkgconf
cd gnome && cargo deb
```

The two are deliberately not one workspace: one is GTK3/cairo and the other is libcosmic, so sharing a lockfile would force a single dependency resolution across two unrelated toolkits.

## Usage

**COSMIC**: Ringlight is an icon in the COSMIC panel. Click it for the controls popup:

- **Enabled** — toggle the ring light on/off
- **Auto (camera)** — automatically enable when a webcam is in use
- **Brightness** — adjust glow intensity (slider)
- **Color temperature** — warmer (amber) ↔ cooler (white) (slider)
- **Glow Size** — Small / Medium / Large
- **Cursor Hole** — Off / Small / Medium / Large (opens a soft gap in the glow at the cursor)
- **Presets** — Warm, Cool, Subtle, Bright

Settings persist across restarts via cosmic-config. To stop the applet, remove it from the panel in COSMIC Settings.

**GNOME**: Ringlight is a system tray icon. Right-click it for Toggle, Auto mode, Brightness up/down, Color temperature and Quit. Settings are in-memory only, so they reset when you quit — cosmic-config is the COSMIC build's persistence layer and has no GNOME equivalent here.

## How it works

- **Overlay rendering**: One transparent Wayland layer-shell surface anchored to all four screen edges. A WGSL fragment shader computes alpha from the distance to the *nearest* edge, so corners are handled without a special case and no seams exist. Peak opacity is capped below fully opaque, so the glow always reads as light rather than paint — content underneath stays readable even at maximum brightness.
- **Click-through**: The surface is created with an empty Wayland input region, so it accepts no pointer input at all and every click passes through to whatever is underneath.
- **Camera detection**: Scans `/proc/*/fd/` every 2 seconds to detect processes that have opened `/dev/video*` devices. No extra packages needed.
- **Cursor tracking**: Uses the compositor's own cursor position via the `ext-image-copy-capture-v1` pointer cursor session. **No permissions are required** — no `input` group membership, no portal prompt, no D-Bus extension — and no frames are ever captured; only position events are read. Because the coordinates come from the compositor, they are exact rather than drifting the way accumulated raw input deltas do.

  This needs a compositor implementing `ext-image-copy-capture-v1`; cosmic-comp does. To check on your machine:

  ```bash
  cargo run --release --example cursor_probe
  ```

  If the protocol is unavailable, Ringlight logs a warning once and runs normally without the cursor hole.

- **Glow size**: Expressed as a fraction of the smaller screen dimension rather than a fixed pixel count, so it looks the same on any display or scale factor.

## How the two builds differ

The "How it works" section above describes the COSMIC build. The GNOME build predates it and shares no code — every layer had to be rewritten for COSMIC, which is why they are two packages rather than one binary with a flag:

| Component | `ringlight-gnome` | `ringlight-cosmic` |
|-----------|--------------|----------------|
| Source | `gnome/` | repository root |
| GUI toolkit | GTK3 + cairo | libcosmic (iced) |
| Panel integration | ksni system tray | COSMIC panel applet |
| Overlay surface | libgtk-layer-shell FFI | iced-sctk layer-shell (one surface) |
| Glow rendering | cairo gradients + DestOut | WGSL fragment shader over wgpu |
| Cursor tracking | D-Bus GNOME extension / `/dev/input` | `ext-image-copy-capture-v1` (no permissions) |
| Settings | in-memory | cosmic-config |
| Async runtime | glib main loop + threads | tokio + iced subscriptions |

The GNOME build bundles its Shell extension from `gnome/gnome-extension/` at compile time and writes it out on first run; that extension is also published to [extensions.gnome.org](https://extensions.gnome.org/) as `ringlight-cursor@ringlight`. The COSMIC build needs no extension.

## License

GPL-3.0
