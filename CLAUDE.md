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
cargo build --release
```

Requires a working Wayland dev environment. libcosmic is pulled from `https://github.com/pop-os/libcosmic.git`.

## Key design decisions

- **No GNOME dependencies**: The original GTK3/cairo/ksni/GNOME-extension stack is fully replaced by libcosmic/iced
- **Raw input for cursor tracking**: On Wayland/COSMIC there's no global cursor position API, so we read `/dev/input/eventN` directly (requires `input` group membership)
- **No persistent settings yet**: Settings are in-memory only. cosmic-config integration can be added later
- **`gnome-extension/` retained**: For reference; not used by the COSMIC build

## API notes

The layer-surface creation in `app.rs` uses `cosmic::iced_sctk::commands::layer_surface`. These import paths can shift between libcosmic releases — check the iced-sctk re-exports if compilation fails. The `Anchor` type uses wlr-layer-shell bitflags (TOP=1, BOTTOM=2, LEFT=4, RIGHT=8).
