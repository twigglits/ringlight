# Ringlight — CLAUDE.md

## Project overview

Desktop ring light overlay for Pop!_OS on the COSMIC desktop (System76). Adds a warm glow around screen edges during video calls, with automatic camera detection.

## Tech stack

- **Language**: Rust (edition 2021)
- **GUI**: libcosmic (iced-based toolkit for COSMIC desktop)
- **Async**: tokio (camera monitoring, subscriptions)
- **Rendering**: iced Canvas widget with strip-based gradient approximation
- **Overlay**: 4 Wayland layer-shell surfaces (one per screen edge) via iced-sctk
- **Panel integration**: COSMIC panel applet (popup for controls)

## Architecture

```
src/
├── main.rs       Entry point: cosmic::applet::run
├── app.rs        COSMIC Application trait impl (panel icon, popup, overlay lifecycle)
├── overlay.rs    Glow rendering: iced Canvas Program with per-edge strip gradients
├── camera.rs     Async /proc/*/fd/ scanner for webcam detection
├── mouse.rs      /dev/input raw event reader for cursor tracking
└── settings.rs   RingLightSettings struct (brightness, color_temp, glow/hole sizes)
```

**Overlay approach**: Four separate layer-shell surfaces (top, bottom, left, right) rather than one full-screen surface. This avoids click-through issues — only the glow edges block pointer input while the center of the screen remains fully interactive.

**Glow rendering**: Multi-pass (5 passes) strip-based gradient with quadratic alpha falloff. Cursor hole uses circle-strip intersection to split strips around the mouse position.

## Build

```bash
# System deps (Pop!_OS / Ubuntu)
sudo apt install -y build-essential cmake libexpat1-dev libfontconfig-dev \
  libfreetype-dev libxkbcommon-dev libwayland-dev libdbus-1-dev \
  libssl-dev libgbm-dev libpipewire-0.3-dev libpulse-dev pkgconf

# Build
cargo build --release
```

Requires Rust stable (1.94+). libcosmic is pulled from `https://github.com/pop-os/libcosmic.git`. First build downloads ~649 crates and takes several minutes.

## Install as COSMIC panel applet

```bash
sudo cp target/release/ringlight /usr/local/bin/
sudo cp ringlight.desktop /usr/share/applications/
```

Then add via **COSMIC Settings → Desktop → Panel → Applets**.

## Key design decisions

- **No GNOME dependencies**: The original GTK3/cairo/ksni/GNOME-extension stack is fully replaced by libcosmic/iced
- **Raw input for cursor tracking**: On Wayland/COSMIC there's no global cursor position API, so we read `/dev/input/eventN` directly (requires `input` group membership)
- **No persistent settings yet**: Settings are in-memory only. cosmic-config integration can be added later
- **`gnome-extension/` retained**: For reference; not used by the COSMIC build

## Verified libcosmic import paths (as of 2026-03-17)

These were validated against the actual pop-os/iced fork and compile cleanly:

```rust
// Layer-surface commands and types
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity, Layer,
};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;

// Popup commands
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};

// Canvas widget (Theme must be cosmic::Theme for COSMIC apps)
use cosmic::iced::widget::canvas::{self, Canvas, Cache, Frame, Geometry};
// impl canvas::Program<Message, cosmic::Theme> for MyProgram { ... }

// Application trait uses Task (not Command)
use cosmic::app::{Core, Task};

// Subscriptions: Subscription::run(|| stream) or Subscription::run_with(id, |&id| stream)
```

The iced fork used by libcosmic renamed `Command` → `Task`. The `Subscription::run` API takes an `fn() -> Stream` (no captures). Use `Subscription::run_with(hashable_id, |&id| stream)` when you need to pass state.

## Status

- Project compiles cleanly (zero errors, zero warnings) as of 2026-03-17
- Not yet runtime-tested on a live COSMIC desktop session
- Future work: cosmic-config persistence, runtime testing, potential shader-based rendering for smoother gradients
