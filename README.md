# Ringlight

A desktop ring light overlay for Pop!\_OS on the COSMIC desktop. Adds a soft, warm glow around the edges of your screen to simulate a ring light during video calls on Google Meet, Teams, Zoom, etc. Automatically activates when your camera turns on.

## Requirements

- Pop!\_OS with COSMIC desktop (or any distro running cosmic-comp)
- Rust toolchain (`rustup` / `cargo`)
- System libraries for libcosmic (Wayland, etc.)

```bash
sudo apt install libwayland-dev libxkbcommon-dev pkg-config libinput-dev
```

> The exact system packages depend on your distro. libcosmic pulls most dependencies through Cargo, but a working Wayland development environment is required.

## Build & Install

```bash
git clone https://github.com/twigglits/ringlight.git
cd ringlight
cargo build --release
```

The binary is at `target/release/ringlight`. Copy it somewhere on your `$PATH`:

```bash
sudo cp target/release/ringlight /usr/local/bin/
```

### Register as a COSMIC panel applet

Copy the desktop entry so COSMIC discovers the applet:

```bash
sudo cp ringlight.desktop /usr/share/applications/
```

Then add "Ringlight" to your COSMIC panel via **Settings → Desktop → Panel → Applets**.

## Usage

Ringlight appears as an icon in the COSMIC panel. Click it to open the controls popup:

- **Enabled** — toggle the ring light on/off
- **Auto (camera)** — automatically enable when a webcam is in use
- **Brightness** — adjust glow intensity (slider)
- **Color temperature** — warmer (amber) ↔ cooler (white) (slider)
- **Glow Size** — Small / Medium / Large
- **Cursor Hole** — Off / Small / Medium / Large (opens a soft gap in the glow at the cursor)
- **Presets** — Warm, Cool, Subtle, Bright

Settings persist across restarts via cosmic-config. To stop the applet, remove it from the panel in COSMIC Settings.

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

## Architecture (COSMIC port)

The original GNOME/GTK3 version used cairo rendering, a ksni system tray, libgtk-layer-shell FFI, and a bundled GNOME Shell extension for cursor tracking. The COSMIC port replaces all of these:

| Component | GNOME version | COSMIC version |
|-----------|--------------|----------------|
| GUI toolkit | GTK3 + cairo | libcosmic (iced) |
| Panel integration | ksni system tray | COSMIC panel applet |
| Overlay surface | libgtk-layer-shell FFI | iced-sctk layer-shell (one surface) |
| Glow rendering | cairo gradients + DestOut | WGSL fragment shader over wgpu |
| Cursor tracking | D-Bus GNOME extension / `/dev/input` | `ext-image-copy-capture-v1` (no permissions) |
| Settings | in-memory | cosmic-config |
| Async runtime | glib main loop + threads | tokio + iced subscriptions |

The bundled `gnome-extension/` directory is retained for reference but is not used by the COSMIC build.

## License

GPL-3.0
