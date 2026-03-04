# Ringlight

A desktop ring light overlay for Ubuntu (Wayland/GNOME). Adds a soft, warm glow around the edges of your screen to simulate a ring light during video calls on Google Meet, Teams, Zoom, etc. Automatically activates when your camera turns on.

## Requirements

- Ubuntu 24.04 LTS (GNOME on Wayland)
- Rust toolchain (`rustup` / `cargo`)
- System libraries:

```bash
sudo apt install libgtk-3-dev libgtk-layer-shell-dev libx11-dev
```

## Build & Install

```bash
git clone https://github.com/twigglits/ringlight.git
cd ringlight
cargo build --release
```

The binary is at `target/release/ringlight`. Copy it somewhere on your `$PATH` if desired:

```bash
sudo cp target/release/ringlight /usr/local/bin/
```

## First-Run Setup

On first launch, Ringlight automatically installs a GNOME Shell extension for accurate cursor tracking. You need to enable it once:

```bash
gnome-extensions enable ringlight-cursor@ringlight
```

Then restart GNOME Shell (log out and back in on Wayland, or Alt+F2 → `r` on X11).

Without the extension, Ringlight falls back to reading raw `/dev/input` events, which requires read permission on the device and provides relative (not absolute) tracking.

## Usage

```bash
ringlight
```

Ringlight runs in the system tray. Right-click the tray icon to:

- **Toggle** the ring light on/off
- **Auto mode** — automatically enable when a webcam is in use
- **Brightness** — adjust glow intensity up/down
- **Color temperature** — warmer (amber) or cooler (white)
- **Quit**

## Permissions

- **Camera detection** reads `/proc/*/fd/` to detect open video devices — no extra packages needed.
- **Raw mouse fallback** needs read access to `/dev/input/eventN`. Add yourself to the `input` group if needed:

```bash
sudo usermod -aG input $USER
```

Then log out and back in.

## License

GPL-3.0
