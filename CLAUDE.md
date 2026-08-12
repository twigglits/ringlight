# Ringlight — CLAUDE.md

## Project overview

Desktop ring light overlay for Pop!_OS on the COSMIC desktop (System76). Adds a warm glow around screen edges during video calls, with automatic camera detection.

**One crate, one extension.** The repository root is the COSMIC build
(`ringlight-cosmic`, a `.deb`). `gnome-extension/` is the GNOME build: a GNOME
Shell extension in GJS, published to extensions.gnome.org, no binary at all.
Everything below documents the COSMIC crate unless it says otherwise; the GNOME
half has its own section at the end.

**There was a `gnome/` Rust crate (GTK3/cairo/ksni) and it is gone** — deleted,
not paused, see "The GNOME build" for the measurements that killed it. Do not
resurrect it from history: on GNOME Wayland it cannot draw the overlay, and the
`.deb` it produced installed cleanly and then did nothing visible.

## Tech stack

- **Language**: Rust (edition 2021)
- **GUI**: libcosmic (iced-based toolkit for COSMIC desktop)
- **Async**: tokio (camera monitoring, subscriptions)
- **Rendering**: iced `shader` widget over wgpu — a WGSL fragment shader
- **Overlay**: one Wayland layer-shell surface via iced-sctk
- **Cursor**: `ext-image-copy-capture-v1` pointer cursor session
- **Panel integration**: COSMIC panel applet (popup for controls)

## Architecture

**Two processes, one binary.** The panel applet cannot own the glow surface; see
"Why the overlay is a separate process" below. The applet spawns its own
executable with `--overlay` and streams settings to the child's stdin.

```
src/
├── main.rs       Entry point: dispatches on --overlay, else cosmic::applet::run
├── app.rs        Applet half: panel icon, popup, camera, owns the child process
├── overlay.rs    Overlay half: windowless iced::daemon whose only surface is the glow
├── ipc.rs        Spawning the child (env sanitising) + the stdin settings channel
├── glow/         GPU glow renderer
│   ├── mod.rs    shader::Program + Primitive + Pipeline
│   └── glow.wgsl Full-screen triangle + edge-falloff fragment shader
├── cursor.rs     Global cursor position via ext-image-copy-capture-v1
├── camera.rs     Async /proc/*/fd/ scanner for webcam detection
├── config.rs     Settings persistence via cosmic-config
└── settings.rs   RingLightSettings (brightness, color_temp, proportional sizes)

examples/
└── cursor_probe.rs   Standalone check that the cursor protocol works here
```

**Overlay approach**: A single layer-shell surface anchored to all four edges, with `input_zone: Some(Vec::new())` (an empty Wayland input region) so it is fully click-through, and `exclusive_zone: -1` so it reaches the true screen edge rather than being pushed inward by the panel. An earlier four-edge-surface design caused corner seams, per-edge cursor misalignment, and falloff clipping at surface boundaries; one surface makes all three structurally impossible.

### Why the overlay is a separate process

**cosmic-panel spawns applets with `WAYLAND_SOCKET` set to a pre-connected fd**,
and libwayland prefers that fd over `WAYLAND_DISPLAY`. A layer surface created
on that connection does **not** get a working empty input region: the
full-screen overlay takes pointer focus across the whole screen and swallows
every click, so nothing can be clicked or dragged while the glow is on.

Established by a controlled comparison, after an earlier fix was committed on
bad evidence and reverted (`f6e26a3` / `f0e4a86`):

| | requests sent | pointer events received |
|---|---|---|
| Run standalone (direct to cosmic-comp) | 1 × `set_input_region`, region with zero `add` calls | 0 enter / 0 button / 0 motion |
| Run as a panel applet | *byte-identical* | 3 enter / 24 button / 9744 motion |

Same binary, same requests, opposite outcomes. The standalone overlay was
confirmed genuinely on screen at the time (265 buffer attaches, 529 frame
callbacks) — a surface that never mapped would also report zero pointer events,
which is the trap that invalidated the first attempt.

Note the process opens many Wayland connections (~40–76 `get_registry` calls),
so **object ids in a `WAYLAND_DEBUG` trace collide across connections**. Do not
conclude anything from an id alone; rely on the differential above.

So the fix is not client-side: the overlay is spawned as a child process with
`WAYLAND_SOCKET` stripped from its environment (`ipc::sanitized_env`), which
connects it to the session compositor directly. `env_clear()` then re-adding is
required — merely overriding leaves the variable inherited.

Two consequences worth keeping:

- **The child is `iced::daemon`, not a `cosmic::app`/applet.** Both of those
  insist on a main window (libcosmic keeps `Settings::no_main_window` private),
  and `applet::run` outside the panel really does map a stray toplevel — 258
  buffer attaches on it in a trace. `iced::daemon` starts with no windows.
- **The child must set a transparent style.** A daemon fills each surface with
  its theme's base background before drawing widgets, and iced's default themes
  are opaque; inheriting that turns the overlay into a full-screen white wash
  that hides the desktop. `overlay::surface_style` pins it to
  `Color::TRANSPARENT`, and a unit test holds that invariant.

The child reads settings as one JSON object per line on stdin, and exits on EOF.
Reusing stdin is deliberate: when the applet dies the pipe closes, so `pkill` on
the applet can never strand a full-screen overlay with no way to dismiss it.

**Glow rendering**: A WGSL fragment shader computes alpha from distance to the *nearest* edge with a quadratic falloff, capped at `MAX_ALPHA = 0.85`, plus a `smoothstep` cutout at the cursor. Taking `min()` over all four edges means corners need no special case.

**Cursor tracking**: cosmic-comp implements `ext-image-copy-capture-v1`, whose pointer cursor session reports cursor position independently of frame capture — no permissions, no input grab, no frames captured, no drift. Verify on any machine with `cargo run --release --example cursor_probe`.

## Build

```bash
# System deps (Pop!_OS / Ubuntu)
sudo apt install -y build-essential cmake libexpat1-dev libfontconfig-dev \
  libfreetype-dev libxkbcommon-dev libwayland-dev libdbus-1-dev \
  libssl-dev libgbm-dev libpipewire-0.3-dev libpulse-dev pkgconf

cargo build --release
cargo test --bins          # note: --bins, the crate has no lib target
```

Requires Rust stable (1.94+). libcosmic is pulled from `https://github.com/pop-os/libcosmic.git`. First build downloads ~649 crates and takes several minutes.

The GNOME half is JavaScript — nothing to build. See "The GNOME build".

## Packaging

One `.deb`, for COSMIC, via `cargo deb` (`cargo install cargo-deb`):

```bash
cargo deb                    # → target/debian/ringlight-cosmic_<ver>-1_amd64.deb
```

`.github/workflows/release.yml` builds it on a `v*` tag and attaches it to a
GitHub release. GNOME is not in that workflow at all — it ships through
`publish-extension.yml` on any push touching `gnome-extension/`.

Facts worth not rediscovering:

- **`Conflicts: ringlight-gnome` stays** even though that package is retired.
  Old GitHub releases still carry it, it ships the same `/usr/bin/ringlight`,
  and it does not work on GNOME Wayland — letting the two coexist would be worse
  than the stale line.
- **`cosmic-panel` is `recommends`, not `depends`.** The applet is useless
  without it, but its package name is not verifiable across Pop!_OS / Ubuntu /
  Debian COSMIC repos, and a wrong `Depends` fails the install outright. Promote
  it to `depends` only after confirming the name on a live COSMIC repo.
- **Version lives in one `Cargo.toml`** now. The extension's `metadata.json`
  version is not a second copy of it — EGO owns that number and assigns its own
  sequence on upload.
- Verify a built package without installing it:
  `apt-get install -s ./path/to.deb` (simulate) and `dpkg-deb -I/-c`.

## Install as COSMIC panel applet

```bash
sudo cp target/release/ringlight /usr/local/bin/
sudo cp ringlight.desktop /usr/share/applications/
```

Or without `sudo`, entirely under `$HOME`:

```bash
mkdir -p ~/.local/bin && cp target/release/ringlight ~/.local/bin/
sed 's|^Exec=.*|Exec='"$HOME"'/.local/bin/ringlight|' ringlight.desktop \
  > ~/.local/share/applications/ringlight.desktop
pkill -x cosmic-panel
```

Then add via **COSMIC Settings → Desktop → Panel → Applets**.

The applet locates the overlay half via `std::env::current_exe()`, so both
layouts work and the two halves are always the same build.

## Working on this project

**cosmic-panel does not respawn applets.** It logs the exit (`ringlight: exited with code 137`) and leaves it dead. `cosmic-session` *does* supervise cosmic-panel, so restart the panel instead:

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
```

To iterate without `sudo` on every build, put a user-level override at `~/.local/share/applications/ringlight.desktop` with `Exec=` pointing at `target/release/ringlight` — it shadows the system entry. Remove it when done.

**Verifying the overlay headlessly.** The glow auto-enables while a camera is in use, so it can be triggered and measured without clicking anything:

```bash
sleep 25 < /dev/video0 &     # auto-mode turns the glow on
sleep 5                       # camera monitor polls every 2s
cosmic-screenshot --interactive=false --notify=false -s ./shots
convert shot.png -format "%[pixel:p{1500,3}]" info:   # y=3 is behind the panel
```

Capture a matched off/on pair and solve `ON = src*a + OFF*(1-a)` per pixel to recover the actual alpha and compare it against the shader maths. This machine has ImageMagick 6, so `convert`, not `magick`.

Take the two shots **as close together as possible** and against a still
desktop: any window that repaints between them (a terminal, a clock) shows up
as glow that is not there, and will wreck the "interior is untouched" check in
particular.

**Driving the overlay half directly** is usually better than going through the
applet — it needs no panel restart, no config edits, and it exits on its own:

```bash
S='{"enabled":true,"brightness":0.35,"color_temp":0.55,"auto_mode":false,
    "glow_size":"Large","hole_size":"Medium"}'
{ printf '%s\n' "$S"; timeout 8 tail -f /dev/null; } \
  | timeout -k 2 20 ./target/release/ringlight --overlay
```

Keep `brightness` low and the window short when a human is watching: at full
brightness this covers the whole screen, and a bug in the surface style can
white the display out completely.

## Key design decisions

- **No GNOME dependencies**: the original GTK3/cairo/ksni/GNOME-extension stack is fully replaced
- **No `/dev/input`**: cursor position comes from the compositor, so no `input` group membership is needed and there is no drift from pointer acceleration. The old approach never worked here at all — the user is not in the `input` group, so the reader silently died and the hole froze at a hardcoded coordinate
- **Proportional sizing**: glow depth is a fraction of the smaller screen dimension (S 0.06 / M 0.10 / L 0.16), so it looks identical at any resolution or scale. Fixed pixel widths made the glow 18% of this display's logical height
- **Physical pixels in uniforms**: `Program::draw` sees *logical* bounds but the fragment shader's `@builtin(position)` is *physical*, so the scale conversion happens in `Primitive::prepare`, the only place `viewport.scale_factor()` is available. Getting this wrong halves the glow width on a 200% display
- **No Quit**: a panel applet's lifetime belongs to cosmic-panel; `process::exit` left a dead icon
- **Overlay in a child process**: not a style choice — an applet's Wayland connection comes from cosmic-panel and does not honour an empty input region, so an in-process overlay swallows every click. See "Why the overlay is a separate process"
- **Child lifetime tied to a pipe, not a pid**: the child exits on stdin EOF, so a `pkill`ed or crashed applet cannot leave a full-screen surface stranded with no way to dismiss it. cosmic-panel SIGKILLs applets (exit 137), so this path is routine, not hypothetical
- **The applet no longer tracks the cursor**: only the glow needs it, and doing it in the child saves the applet a Wayland connection
- **`gnome-extension/` is the whole GNOME product**, not a helper for a binary. `publish-extension.yml` uploads it to extensions.gnome.org on any push that touches it. The COSMIC build does not use it. Its `uuid` (`ringlight-cursor@ringlight`) is the extensions.gnome.org identity — changing it makes a resubmission a brand-new listing instead of a new version, so leave it alone even though the name no longer mentions cursors. Do *not* hand-maintain the `version` field: EGO assigns its own sequence number on upload and regenerates the metadata it serves (the repo sat at `1` while EGO was already serving `4`)

## The GNOME build

`gnome-extension/` — GJS, GNOME Shell 45+, published to EGO as
`ringlight-cursor@ringlight`. It replaced a Rust GTK3 crate that could not work.

```
gnome-extension/
├── extension.js   Panel button, overlay actor, wiring; the Extension subclass
├── glow.js        Cairo painting. No shell imports, so it renders offline
├── camera.js      CameraWatcher: sliced /proc sweep for /dev/video* holders
├── prefs.js       Adw preferences window
├── schemas/       GSettings schema (compiled by EGO on upload, not committed)
└── tools/         check-glow.sh, check-camera.js, render-glow.js
```

### Why the glow has to be drawn by the shell

A GTK3 window cannot be an overlay under mutter. Measured with `WAYLAND_DEBUG=1`
on GNOME Shell 46 / Ubuntu, from the retired crate's fallback path:

- The registry advertises `xdg_wm_base` and `gtk_shell1` and **no
  `zwlr_layer_shell_v1`**. mutter has never implemented it and the maintainers
  have repeatedly declined to; `gtk_layer_is_supported()` returns 0.
- The fallback creates a plain `xdg_toplevel` — `set_title("Ringlight")`,
  `set_app_id`, alt-tabbable, placed wherever mutter likes.
- `set_keep_above`, `set_type_hint(Dock)`, `stick()` and `move_(0, 0)` emit
  **zero Wayland requests between them**. They are X11-only; on the GTK3 Wayland
  backend they are silent no-ops. There is no xdg-shell request to position a
  toplevel, by design.
- `wl_surface.set_input_region` *is* sent, so click-through was the one part
  that worked.
- `gdk_monitor_get_scale_factor: assertion 'GDK_IS_MONITOR (monitor)' failed`,
  then `could not detect monitor size, using 1920x1080 fallback` — GTK3 Wayland
  has no primary monitor, so the window was also the wrong size.

`Main.layoutManager.addTopChrome(actor, {affectsInputRegion: false})` gets all
of it — above windows, screen-sized, click-through — because the shell is the
compositor. That is the only route on GNOME.

`trackFullscreen` is deliberately `false`: a fullscreen video call is exactly
when the light is wanted.

### The shell does no recurring work — this is the whole design

The published EGO v4 (`pk 73938`, 43 lines, identical to commit `5200204`) has
no actor, no timer and no repaint handler at all: it exports one D-Bus
`GetPosition` and the Rust binary drew the glow. It could not light anything on
GNOME Wayland, but it never cost a frame either. **That is the constraint this
build now keeps: nothing in the shell runs on a clock or on pointer motion.**

The first shell-drawn version broke it and made the machine crawl, pointer
included. Timing the `repaint` handler inside a real GNOME Shell 46 on a
2560x1600 monitor while the pointer swept an edge for 10s:

| | repaints in 10s | mean repaint | main loop spent painting |
|---|---|---|---|
| full-size surface, pointer cut-out | 50 | **67.66ms** | 3.38s of 10s |
| 512px surface, cached glow, clipped cut-out | 200+ | 0.25ms | 0.05s |
| **now: static glow, no cut-out** | **0** | 0.54ms (startup only) | ~0 |

67ms is four frame budgets, and mutter moves the pointer on that same loop —
hence the lag. Note the middle row: even at 0.25ms it is still 200 repaints and
200 texture uploads that buy nothing the user asked for. The fix that holds is
not a faster repaint, it is **no repaint**. Whole-process CPU while sweeping the
pointer, same harness: 24.0% with the glow off, 24.4% with it on.

What that costs and what it bought:

- **The pointer cut-out is gone**, along with `hole-size`, the 20Hz
  `global.get_pointer()` timer and every repaint. It was the only thing in the
  extension that changed while the glow was up. Putting it back means putting
  the timer back; do not, unless someone measures a version that does not.
- **Scale the actor, don't paint big.** The surface is 512px on its long side
  (`RENDER_MAX`) and the actor is scaled to the monitor. Verified in a headless
  shell: a 512x320 surface reports `transformed=1280x800+0+0` on a 1280x800
  monitor. Against a full-size render the upscale costs a *max* alpha error of
  0.016 (mean 0.001), because every gradient here is far smoother than the pixel
  grid. This matters less now that repaints are rare, but it keeps a settings
  change to half a millisecond instead of 67, which is what a slider drag needs.
  Measure the error with clamp-to-edge sampling (`Cairo.Extend.PAD`), which is
  what Cogl does — cairo's default `NONE` blends the border with transparent
  black and reports a fake 0.64 error at the corners.
- **`glow.js` takes no pointer state.** Nothing it draws can change while the
  glow is up, which is what makes "paint once" safe rather than a bug waiting to
  happen.

The one recurring cost left in the shell is the camera sweep, and it only runs
when `auto-mode` is on. `/proc/<pid>/fd` mtime does *not* change when a process
opens an fd (measured), so there is no cheap way to skip the sweep — the slicing
below is the mitigation.

To re-measure any of this without logging out, drive the pointer through
Mutter's own API — `org.gnome.Mutter.RemoteDesktop`, `CreateSession` then
`Session.NotifyPointerMotionRelative` (*not* `NotifyPointerMotion`, which does
not exist). Two traps:

- **A headless shell does not paint** unless something consumes the frames, and
  a shell that is not painting makes any renderer look fast. Tell it apart by
  changing the virtual monitor size: real painting scales with it. Start a
  consumer with `org.gnome.Shell.Screencast.Screencast`.
- Even then, headless runs on llvmpipe, so whole-process CPU is dominated by
  software compositing and is ±4 points run to run — the old and new code came
  out 27.6% vs 26.5%, which says nothing. Time the `repaint` handler itself and
  `console.log` it instead; that is the number above.
- **`dbus-run-session` starts a second `dconf-service` against the same
  `~/.config/dconf/user`.** After a few nested runs the session's own service is
  desynced: `dconf write` reports success, `dconf read` returns the old value and
  `dconf watch` shows nothing. It is not the extension writing settings back.
  `pkill -f libexec/dconf-service` (it is D-Bus activated, nothing is lost) and
  the writes land again.

### Camera detection is a sliced /proc sweep

`Shell.CameraMonitor` exists and exposes `cameras-in-use`, and it is the wrong
tool: gnome-shell links libpipewire and that property only sees PipeWire
consumers, which most browsers and conferencing apps are not — they open
`/dev/video*` directly. Using it would silently break the feature's whole point.

So `camera.js` walks `/proc/*/fd` like the Rust version did, but this runs
*inside the compositor*, where the measured 21ms full sweep would drop a frame
every 2s poll. It is spread over `GLib.idle_add` callbacks with a 2ms budget
each. Notes:

- The budget is time, not a process count: fd counts per process are wildly
  uneven (worst single process here, 148 fds, 1.09ms).
- Listing `/proc` is itself up to ~4ms cold, so it waits for the first slice
  rather than running on the timer.
- Slices hold to 2.0ms, but the occasional ~9ms outlier still lands — /proc I/O
  and SpiderMonkey GC, not the loop. `check-camera.js` bounds the worst
  main-loop gap at 18ms, well under the ~21ms an unsliced sweep costs, so it
  catches a regression to blocking without flaking on the tail.
- `CameraWatcher`'s second constructor argument is a devices-list seam, there so
  the check can force a full sweep on a machine with no webcam plugged in.

### Testing it without logging out

GNOME Shell cannot reload an extension mid-session under Wayland, so there is no
edit-reload loop — every change to `extension.js` costs a logout. The two pieces
with real logic in them are therefore kept runnable from a terminal:

```bash
gnome-extension/tools/check-glow.sh            # samples a rendered PNG against the maths
gjs -m gnome-extension/tools/check-camera.js   # sweep correctness + main-loop cost
gjs -m gnome-extension/tools/render-glow.js /tmp/g.png   # eyeball it
```

A **headless shell does load the extension**, which the checks above cannot:

```bash
dbus-run-session -- bash -c 'gnome-shell --headless --virtual-monitor 1280x800 >/tmp/shell.log 2>&1 &
  sleep 18; gnome-extensions info ringlight-cursor@ringlight; kill %1'
```

`State: ACTIVE` and no `JS ERROR` in `/tmp/shell.log` covers everything up to
pixels: `enable()`, the settings and monitor signals, the camera watcher. It
reads `~/.local/share/...`, not the repo, so install first. Screenshots are not
available (`org.gnome.Shell.Screenshot` answers `AccessDenied` to plain D-Bus
callers), so to check geometry, `console.log` it from the extension — that lands
in the shell log, and is how the scaled overlay was confirmed to cover the
monitor.

`glow.js` imports nothing from the shell purely so this stays possible. Keep it
that way. For syntax, `node --input-type=module --check < file.js` parses GJS
ESM fine and is faster than a logout.

Installing a working copy locally:

```bash
EXT=~/.local/share/gnome-shell/extensions/ringlight-cursor@ringlight
mkdir -p "$EXT/schemas"
cp gnome-extension/*.js "$EXT/"
cp gnome-extension/schemas/*.gschema.xml "$EXT/schemas/"
glib-compile-schemas "$EXT/schemas/"        # EGO does this on upload; local installs must
sed 's/"version": 1/"version": 5/' gnome-extension/metadata.json > "$EXT/metadata.json"
```

The `version` bump is not cosmetic: EGO serves version 4, and with automatic
updates on, the Extensions app reinstalls EGO's copy over a lower-numbered local
one at login — silently reverting whatever you were testing.

### Packing

`gnome-extensions pack` (and the EGO upload action, which mirrors it) includes
only `metadata.json`, `extension.js`, `prefs.js`, `stylesheet.css` and the
schema. **`glow.js` and `camera.js` must be listed as `extra-source` or they are
dropped from the zip and the extension fails to load.** `publish-extension.yml`
passes them; check that first if a published version breaks and the local copy
does not.

## Verified libcosmic import paths (as of 2026-08-06)

Validated against the pinned pop-os/libcosmic revision and compiling cleanly:

```rust
// Layer-surface commands and types
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity, Layer,
};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;

// Popup commands
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};

// Shader widget (requires libcosmic's "wgpu" feature)
use cosmic::iced_widget::shader::{self, Shader, Viewport};
use cosmic::iced_wgpu::wgpu;   // MUST come from here, not a direct wgpu dependency,
                               // or the types will not match iced's (wgpu 27.0.1)

// Application trait uses Task (not Command)
use cosmic::app::{Core, Task};

// The overlay half bypasses libcosmic's app layer entirely, for a windowless
// process. `daemon` is re-exported through libcosmic's own iced:
cosmic::iced::daemon(boot, update, view)   // boot: Fn() -> (State, Task<Message>)
    .title(|_state, _id| String::new())    // update: Fn(&mut State, Message) -> Task
    .style(|_state, _theme| style)         // view: Fn(&State, window::Id) -> Element
    .subscription(subscription)
    .run()
```

`shader::Primitive` in this revision differs from the published docs: it carries an associated `Pipeline` type and `fn draw(&self, pipeline, render_pass) -> bool`. `Pipeline::new(device, queue, format)` is called once, lazily.

The `CosmicConfigEntry` derive emits *unqualified* `cosmic_config::` paths and a bare `CosmicConfigEntry`, so both names must be imported where the derive is applied:

```rust
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
```

**Silent-failure trap:** iced's renderer is a fallback pair (wgpu primary, tiny-skia secondary). On the tiny-skia path, custom shader primitives are *discarded* with only `log::warn!("Custom shader primitive is not supported with this renderer.")`. If the glow ever renders as nothing, check for that line first.

## Status

- Compiles clean; `cargo clippy --all-targets` clean; 24 unit tests pass
- Runtime-verified on Pop!_OS 24.04 COSMIC, originally on eDP-1 (3000x2000 @ 200%) and re-verified on DP-1 (3440x1440): glow falloff, corner uniformity, cursor-hole alignment and settings persistence were all confirmed by sampling screenshots against the shader maths, not by eye. Measured alpha tracks the shader to a mean error of ~0.01–0.03 across the falloff band, with all four edges agreeing
- **Click-through is fixed and confirmed** on the two-process design: clicks, drags and window moves all work with the glow on. The single-process applet version swallowed every click; see "Why the overlay is a separate process"
- The COSMIC `.deb` builds and passes `apt-get install -s`; `dpkg-deb -c` shows binary, desktop entry and licence in the right places. It has never been runtime-tested as an installed package, only built and simulated
- **This machine now runs Ubuntu GNOME, not COSMIC** (`XDG_CURRENT_DESKTOP=ubuntu:GNOME`, no `cosmic-*` in apt). The COSMIC runtime measurements above were taken when it ran Pop!_OS COSMIC and **cannot be re-run here** — the screenshot/alpha-sampling recipes need cosmic-comp. The GNOME half is the testable one now
- **GNOME extension**: `check-glow.sh` passes — measured alpha tracks the shader maths to 0.002 at the falloff band, edges reach 1.0 and the interior is 0.0. `check-camera.js` passes, including the main-loop-stall bound. It loads `State: ACTIVE` with no JS errors in a headless GNOME Shell 46, through enable/disable and auto-mode toggles, and the scaled overlay was confirmed there to transform to exactly the monitor rectangle. No webcam was attached to this machine, so auto-mode is not verified end to end; `journalctl -f -o cat /usr/bin/gnome-shell` is where a load failure in a real session would show
- **The first shell-drawn version made the whole desktop crawl**, pointer included: 67.66ms of main-loop work per pointer step. The glow is now painted once and never repainted while it is up — 0 repaints during 10s of pointer motion, and whole-process CPU indistinguishable from the glow being off. See "The shell does no recurring work". The pointer cut-out was dropped to get there
- The EGO listing is id 9483, `ringlight-cursor@ringlight`, serving version 4 at the time of the rewrite
- Design and plan: `docs/superpowers/specs/` and `docs/superpowers/plans/`
