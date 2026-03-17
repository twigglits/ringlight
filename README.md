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
- **Cursor Hole** — Off / Small / Medium / Large (dims glow near the cursor)
- **Presets** — Warm, Cool, Subtle, Bright
- **Quit** — exit the applet

## How it works

- **Overlay rendering**: Four transparent Wayland layer-shell surfaces are created on the Overlay layer, one per screen edge. Each surface renders its portion of the glow using an iced Canvas widget with strip-based gradient rendering and a multi-pass approach for brightness.
- **Camera detection**: Scans `/proc/*/fd/` every 2 seconds to detect processes that have opened `/dev/video*` devices. No extra packages needed.
- **Cursor tracking**: Reads raw input events from `/dev/input/eventN` to track mouse position. Requires read access to the input device — add yourself to the `input` group if needed:

```bash
sudo usermod -aG input $USER
```

Then log out and back in.

## Architecture (COSMIC port)

The original GNOME/GTK3 version used cairo rendering, a ksni system tray, libgtk-layer-shell FFI, and a bundled GNOME Shell extension for cursor tracking. The COSMIC port replaces all of these:

| Component | GNOME version | COSMIC version |
|-----------|--------------|----------------|
| GUI toolkit | GTK3 + cairo | libcosmic (iced) |
| Panel integration | ksni system tray | COSMIC panel applet |
| Overlay surface | libgtk-layer-shell FFI | iced-sctk layer-shell |
| Glow rendering | cairo gradients + DestOut | iced Canvas strips |
| Cursor tracking | D-Bus GNOME extension / `/dev/input` fallback | `/dev/input` only |
| Async runtime | glib main loop + threads | tokio + iced subscriptions |

The bundled `gnome-extension/` directory is retained for reference but is not used by the COSMIC build.

## License

GPL-3.0
