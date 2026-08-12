# Ringlight

A desktop ring light overlay. Adds a soft, warm glow around the edges of your screen to simulate a ring light during video calls on Google Meet, Teams, Zoom, etc. Automatically activates when your camera turns on.

Ringlight ships **two ways**, because a screen-edge overlay has to be built against the compositor you are actually running. Pick the one matching your desktop.

| Your desktop | Get it from | Controls |
|---|---|---|
| GNOME (Ubuntu, Fedora, Debian…) | [extensions.gnome.org](https://extensions.gnome.org/extension/9483/ringlight-cursor-tracker/) | panel menu + extension settings |
| COSMIC on Pop!\_OS | `ringlight-cosmic.deb` | cosmic-panel applet |

## Install

### GNOME

Install **Ringlight** from [extensions.gnome.org](https://extensions.gnome.org/extension/9483/ringlight-cursor-tracker/), then log out and back in. That is the whole install — there is no package and no binary to run.

A lamp icon appears in the top bar. It is on by default whenever an app opens your webcam.

> There is no `.deb` for GNOME, and a GNOME Ringlight that is not the extension will not work. mutter does not implement `zwlr_layer_shell_v1`, and a plain application window cannot be made always-on-top, screen-sized *and* click-through under Wayland — `set_keep_above`, `move()` and the Dock window-type hint are all X11-only no-ops there. Only the shell itself can put that surface on screen, so only an extension can draw the glow.

### Pop!\_OS (COSMIC desktop)

Grab `ringlight-cosmic.deb` from the [latest release](https://github.com/twigglits/ringlight/releases/latest).

```bash
sudo apt install ./ringlight-cosmic.deb
```

Then add "Ringlight" to your panel via **Settings → Desktop → Panel → Applets**. No extension and no permissions are needed; cursor position comes from the compositor.

## Build from source

```bash
git clone https://github.com/twigglits/ringlight.git
cd ringlight
```

**COSMIC** (repository root) needs a Rust toolchain. `cargo deb` produces the same package the release workflow does; plain `cargo build --release` gives you just the binary.

```bash
sudo apt install build-essential cmake libexpat1-dev libfontconfig-dev \
  libfreetype-dev libxkbcommon-dev libwayland-dev libdbus-1-dev \
  libssl-dev libgbm-dev libpipewire-0.3-dev libpulse-dev pkgconf
cargo deb                      # or: cargo build --release
```

**GNOME** (`gnome-extension/`) is JavaScript, so there is nothing to compile — install it in place:

```bash
ln -s "$PWD/gnome-extension" ~/.local/share/gnome-shell/extensions/ringlight-cursor@ringlight
glib-compile-schemas gnome-extension/schemas/
gnome-extensions enable ringlight-cursor@ringlight
```

Log out and back in. GNOME Shell cannot reload an extension mid-session under Wayland, so every change to `extension.js` costs a logout — which is why the two pieces with real logic in them are checkable from a terminal instead:

```bash
gnome-extension/tools/check-glow.sh          # renders the glow, asserts its shape
gjs -m gnome-extension/tools/check-camera.js # /proc sweep: correctness and main-loop cost
```

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

**GNOME**: Ringlight is a lamp icon in the top bar. Its menu holds the on/off switch, **Follow camera**, and a brightness slider; colour, glow size and the pointer cut-out are under **Settings…** (or `gnome-extensions prefs ringlight-cursor@ringlight`). Everything persists in GSettings.

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

The "How it works" section above describes the COSMIC build. The GNOME build shares no code with it: on GNOME the glow is drawn by the shell itself, so there is no process, no toolkit and no Wayland surface of our own anywhere in the picture.

| Component | GNOME extension | `ringlight-cosmic` |
|-----------|-----------------|----------------|
| Source | `gnome-extension/` | repository root |
| Language | JavaScript (GJS) | Rust |
| Runs in | the compositor | its own process |
| Panel integration | `PanelMenu.Button` | COSMIC panel applet |
| Overlay surface | shell chrome, `affectsInputRegion: false` | iced-sctk layer-shell (one surface) |
| Glow rendering | cairo gradients + `DEST_OUT` on a `St.DrawingArea` | WGSL fragment shader over wgpu |
| Cursor tracking | `global.get_pointer()` | `ext-image-copy-capture-v1` (no permissions) |
| Camera detection | `/proc` sweep, sliced across idle callbacks | `/proc` sweep on a worker thread |
| Settings | GSettings | cosmic-config |

Both draw the same glow: five stacked edge gradients, each wider and dimmer than the last, with a radial `DEST_OUT` cut-out at the pointer. Sizes are fractions of the smaller screen dimension in both, so a display's resolution and scale factor do not change how the glow looks.

The split is forced, not stylistic. cosmic-comp implements `zwlr_layer_shell_v1` and mutter does not, and mutter will not honour the X11-era escape hatches (`set_keep_above`, window positioning, the Dock type hint) that the glow would otherwise need. Under GNOME the only code that can put a click-through, always-on-top, screen-sized surface up is code running inside the shell.

## License

GPL-3.0
