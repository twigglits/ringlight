# Ringlight COSMIC overlay — redesign

Date: 2026-08-06
Branch: `desktop/cosmic`
Status: approved

## Problem

The COSMIC implementation on `desktop/cosmic` compiles cleanly and the applet runs, but
the overlay does not work. The user reports three symptoms: solid opaque bars instead of a
glow, a glow far too thick, and a dark hole frozen in one spot.

Eight defects were confirmed against the running Pop!_OS 24.04 COSMIC session
(cosmic-comp, eDP-1 at 3000x2000 physical / 1500x1000 logical, 200% scale).

| # | Defect | Evidence |
|---|--------|----------|
| 1 | Cursor tracking never runs | `/dev/input/event*` is `root:input 0660`; the user is not in `input`. `File::open` fails, the thread returns, the `watch` sender drops, the subscription ends silently. The hole stays at the hardcoded `(960, 540)`. |
| 2 | Screen size hardcoded `1920x1080` | Actual logical size is 1500x1000. Bottom/right cursor mapping is off by 80/420px and the clamp range is wrong. |
| 3 | Alpha saturates to opaque | Five passes sum to `brightness * 0.85 * (1.0+0.9+0.8+0.7+0.6)` = `3.4 * brightness` = 2.38 at the default 0.7, clamping to 1.0. |
| 4 | Hard clipped seam | Surfaces are `glow_w` (180px) deep but passes widen to `glow_w * 2` = 360px, so passes 2-5 never reach zero inside the surface and alpha stops abruptly at the boundary. |
| 5 | Banding, 800 draw ops per frame | 40 strips x 5 passes x 4 surfaces, approximating a gradient the GPU can compute directly. |
| 6 | Redraw storm (latent) | One `MouseMoved` message per `EV_REL` event, each calling `clear_all()` and repainting four surfaces. Dormant only because defect 1 kills the source. |
| 7 | No persistence; `Quit` calls `process::exit(0)` | Settings reset whenever cosmic-panel respawns the applet; exiting leaves a dead panel icon. |
| 8 | Glow does not reach the screen edge | `exclusive_zone: 0` means "move me so I don't occlude surfaces with a positive exclusive zone", so cosmic-panel pushes the top surface inward. `main` correctly used `-1`. |

Defects 2, 4, 5 and 8 are consequences of the four-edge-surface shape, not independent bugs.

## Research: reading the cursor position on COSMIC

Getting a global cursor position was assumed impossible on Wayland without `/dev/input`
access or a compositor extension. That assumption is wrong for cosmic-comp.

cosmic-comp implements `ext-image-copy-capture-v1`, whose
`create_pointer_cursor_session(source, wl_pointer)` request returns a session that emits
`position` events independently of frame capture. Verified empirically with a probe on the
live session (retained as `examples/cursor_probe.rs`):

```
ext_output_image_capture_source_manager_v1       PRESENT
ext_image_copy_capture_manager_v1                PRESENT
  [event] enter
  [event] position x=1480 y=1174     <- immediately on session creation
  [event] position x=1481 y=1172     <- then streaming on cursor motion
  ... 40+ samples ...
```

Properties that make this strictly better than anything `main` does:

- **No permissions.** Plain client, no portal, no `input` group, no D-Bus extension.
- **No input grab.** The overlay stays click-through; `wl_pointer` focus is never taken.
- **No drift.** True compositor coordinates, with pointer acceleration already applied,
  unlike accumulated relative deltas.
- **No polling.** Event-driven, and the position arrives on session creation rather than
  on first movement.
- **No capture cost.** Zero buffers attached, zero frames captured.

Coordinates are physical buffer pixels (0-3000 x 0-2000) and report the cursor *hotspot*,
which is the pointer tip, so no hotspot correction is needed.

Two negative findings worth recording so they are not re-investigated:
`zwlr_virtual_pointer` is absent from cosmic-comp, so the `wl-find-cursor` technique
(nudging the pointer to force a motion event) would not work here; and `cosmic-protocols`
contains no cursor-position protocol of its own.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Renderer | wgpu shader widget | User-selected over Canvas gradients. Exact falloff, no banding, soft cursor hole, one draw call. |
| Surfaces | One full-screen | The shader needs a single coordinate space; four surfaces reintroduce defects 2 and 4. |
| Cursor source | `ext-image-copy-capture` pointer cursor session | Verified working; no permissions, no drift. |
| Monitors | Active output only | Matches the user's hardware and what `main` did. |
| Persistence | cosmic-config | User-requested addition. |
| Quit | Removed | A panel applet's lifetime belongs to cosmic-panel. |

## Architecture

### Surface model

One layer surface, created when active and destroyed when inactive:

```rust
SctkLayerSurfaceSettings {
    layer: Layer::Top,
    anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
    size: Some((None, None)),          // anchored to opposite edges => fills the output
    exclusive_zone: -1,                // reach the true screen edge (fixes defect 8)
    input_zone: Some(Vec::new()),      // empty wl_region => click-through (verified)
    keyboard_interactivity: KeyboardInteractivity::None,
    output: IcedOutput::Active,
    ..Default::default()
}
```

The shader's `bounds` is the logical screen, so no screen dimension is stored in
application state. Defect 2 becomes inexpressible.

`Layer::Top` is retained from the current code. The existing comment claims cosmic-comp
forces an input grab on `Layer::Overlay`; that claim is untested and is not relied upon,
since `Top` is correct regardless — the glow should sit above windows but below the OSD
and lock screen.

### Cursor tracking — `cursor.rs`

Replaces `mouse.rs`. A dedicated thread owns its own `wayland_client::Connection`,
independent of the connection iced-sctk owns, and runs its own event queue.

Responsibilities:

1. Bind `wl_seat` (wait for the pointer capability before `get_pointer`), `wl_output`,
   `ext_output_image_capture_source_manager_v1`, `ext_image_copy_capture_manager_v1`.
2. Create an output capture source and a pointer cursor session. Never attach a buffer.
3. Track the output's physical size from `wl_output.mode`, swapping width/height when the
   `transform` is 90 or 270.
4. Publish normalized `[0.0, 1.0]` positions on a `tokio::sync::watch` channel.

Normalizing rather than converting to logical pixels keeps the value independent of the
output scale, so fractional scaling cannot reintroduce defect 2. The shader multiplies
back up by `bounds`.

`enter`/`leave` drive a `visible` flag published alongside the position; on `leave` the
hole is suppressed rather than left stranded at a stale coordinate.

Failure is non-fatal and reported once: if either global is missing, the thread logs and
exits, and the app forces `hole_radius = 0` so the glow still works. If the connection
drops, the thread retries with capped exponential backoff.

### Rate limiting

The subscription bridging the `watch` channel into `Message::CursorMoved` emits at most
one message per ~16ms, and drops updates whose movement is sub-pixel. This addresses
defect 6, which the current code would hit the moment cursor input started working.

### Rendering — `glow/`

`glow/mod.rs` implements `shader::Program<Message>` with an associated
`Primitive = GlowPrimitive` and `Pipeline = GlowPipeline`.

- `GlowPipeline::new(device, queue, format)` builds the render pipeline, uniform buffer
  and bind group once.
- `GlowPrimitive::prepare` writes the uniform buffer.
- `GlowPrimitive::draw` records into iced's existing render pass and returns `true`.
- Geometry is a full-screen triangle generated from `vertex_index`; no vertex buffer.
- Blend state is `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`, matching iced's
  compositing, and the shader outputs premultiplied colour.

Uniforms, padded to 16-byte alignment:

**Coordinate space.** `Program::draw` receives `bounds` in *logical* points, but the
fragment shader's `@builtin(position)` is in *physical* framebuffer pixels. Every length in
the uniform block is therefore in physical pixels: multiply by
`viewport.scale_factor()` in `prepare`. Getting this wrong renders the glow at half the
intended width on this 200% display, so it is stated explicitly rather than left implied.

| Field | Type | Notes |
|-------|------|-------|
| `resolution` | `vec2<f32>` | surface size in **physical** pixels (`bounds.size * scale_factor`) |
| `cursor` | `vec2<f32>` | normalized `[0,1]`; multiplied by `resolution` in the shader, so scale-independent |
| `color` | `vec3<f32>` | from `color_temp` |
| `brightness` | `f32` | 0.0-1.0, unitless |
| `glow_width` | `f32` | **physical** pixels |
| `hole_radius` | `f32` | **physical** pixels; 0.0 disables the hole |
| `hole_softness` | `f32` | unitless fraction of the radius inside which the hole is fully open |

Fragment shader core:

```wgsl
let d = min(min(p.x, size.x - p.x), min(p.y, size.y - p.y));   // dist to nearest edge
var a = pow(max(0.0, 1.0 - d / glow_width), 2.0);               // reaches exactly 0
a = a * brightness * MAX_ALPHA;                                  // MAX_ALPHA = 0.85
if (hole_radius > 0.0) {
    a = a * smoothstep(hole_radius * hole_softness, hole_radius, distance(p, cursor_px));
}
return vec4<f32>(color * a, a);                                  // premultiplied
```

This single expression retires four defects: one alpha term instead of five summed (3),
falloff reaching zero inside the surface (4), no strips so no banding (5), and `min()`
across all four edges so corners need no special case and no seams exist.

`MAX_ALPHA = 0.85` caps peak opacity so the glow always reads as light rather than paint.

### Proportional glow width — `settings.rs`

Fixed pixel widths are why the glow reads as far too thick: 180px is 18% of a 1000px
logical height. Width becomes a fraction of `min(width, height)`:

| Size | Fraction | On 1500x1000 logical | Physical at 200% |
|------|----------|----------------------|------------------|
| Small | 0.06 | 60px | 120px |
| Medium | 0.10 | 100px | 200px |
| Large | 0.16 | 160px | 320px |

Hole radius becomes proportional on the same basis: Small 0.08, Medium 0.14, Large 0.22.

The fraction is computed against logical bounds; the resulting length is converted to
physical pixels before it reaches the uniform block, per the coordinate-space note above.

### Persistence — `config.rs`

Wraps `cosmic_config` keyed on `APP_ID`. Settings load at startup, falling back to
defaults when absent or malformed, and writes are debounced so slider drags do not
generate one write per frame.

### Lifecycle

`Message::Quit` and `process::exit(0)` are removed. The Enabled toggle is the off switch;
removing the applet from the panel is how it is quit.

### Module layout

```
src/
├── main.rs      entry point
├── app.rs       Application impl: panel icon, popup, surface lifecycle, subscriptions
├── settings.rs  RingLightSettings + proportional derived values
├── config.rs    cosmic-config persistence            [new]
├── cursor.rs    capture-session cursor thread        [replaces mouse.rs]
├── camera.rs    unchanged
└── glow/
    ├── mod.rs   shader::Program + Primitive + Pipeline
    └── glow.wgsl
examples/
└── cursor_probe.rs   standalone protocol verification
```

`mouse.rs` and `overlay.rs` are deleted.

New dependencies: `wayland-client`, `wayland-protocols` (features `client`, `staging`),
`cosmic-config`, `bytemuck`. All except `cosmic-config` and `bytemuck` are already
transitive dependencies at compatible versions (`wayland-client` 0.31, `wayland-protocols`
0.32.11).

## Data flow

```
camera.rs  --(poll 2s)-->  Message::CameraStateChanged --+
cursor.rs  --(watch, rate-limited)--> Message::CursorMoved --+
popup UI   --------------------------> Message::Set*        --+
                                                              |
                                                              v
                                                    app.rs update()
                                                     |          |
                                    settings changed |          | active changed
                                                     v          v
                                              config.rs    create/destroy
                                              (debounced)   layer surface
                                                     |
                                                     v
                                          view_window -> Shader(GlowProgram)
                                                     |
                                                     v
                                       GlowPrimitive::prepare -> uniforms
                                       GlowPrimitive::draw    -> render pass
```

`is_active() = settings.enabled || (settings.auto_mode && camera_active)`, unchanged.

## Error handling

| Failure | Detection | Behaviour |
|---------|-----------|-----------|
| wgpu falls back to tiny-skia | Renderer type at startup | Log a clear warning. Custom primitives do not render on tiny-skia, so the glow will not appear; this is visible and explained rather than silent. |
| Capture-source globals missing | Registry bind returns none | Log once, exit the cursor thread, force `hole_radius = 0`. Glow still works. |
| Cursor session rejected | Protocol error on roundtrip | As above. |
| Wayland connection drops | Dispatch error | Retry with capped exponential backoff. |
| Cursor leaves the output | `leave` event | Suppress the hole rather than leave it at a stale position. |
| cosmic-config unavailable | Load/save error | Log, run with in-memory defaults. |
| Layer surface creation fails | No `view_window` callback | Log; applet stays responsive. |

No failure mode takes down the applet or the panel.

## Testing

Unit-testable pure functions, extracted deliberately so they can be tested off-GPU:

- `falloff(d, glow_width)` — 1.0 at d=0, exactly 0.0 at d>=glow_width, monotonic.
- `glow_width_px(size, bounds)` and `hole_radius_px(size, bounds)` — proportional scaling.
- `normalized_to_pixels(cursor, bounds)` — round-trip against known values.
- `glow_color(color_temp)` — endpoints match the documented warm/cool anchors.
- Settings serde round-trip through cosmic-config's representation.
- Rate limiter: sub-pixel and sub-16ms updates are dropped; larger ones pass.

Shader correctness is not unit-testable; it is covered by the spike and manual checks.

Manual verification on the live COSMIC session:

1. A spike rendering one flat translucent quad through `shader::Program` proves the
   pipeline works inside the applet process **before** any glow math is written.
2. Toggle on: soft glow on all four edges reaching the true screen edge, including behind
   the panel, with no hard inner line and no corner seams.
3. Clicks pass through the glow to windows underneath.
4. The hole follows the cursor smoothly and with correct alignment at all four edges and
   the corners.
5. Brightness at 1.0 never fully hides content underneath.
6. Camera auto-mode toggles the overlay when a call starts and ends.
7. Settings survive `pkill ringlight` and the panel respawn.

## Risks

**The shader API is the main risk.** `shader::Program`, `Primitive` and `Pipeline` are the
least stable surface in libcosmic, and this project tracks libcosmic from git `main`. The
`Primitive` trait in the pinned revision (`0bb006c`) already differs from the widely
documented form — it carries an associated `Pipeline` type and a
`draw(&self, pipeline, render_pass) -> bool` method. Mitigation: the spike above is the
first implementation step, so a breaking mismatch surfaces in an hour rather than after
the renderer is complete. If the spike fails, the fallback is Canvas with native
`Gradient::Linear` plus an `EvenOdd` path for the hole; both APIs were verified present.

**A full-screen surface prevents direct scanout** while the glow is on. Small in practice:
the overlay only exists during calls, and the four-edge version overlaps a fullscreen
video anyway, so scanout is lost either way.

**`ext-image-copy-capture` is a staging protocol** and may change. It is implemented and
working in the shipped cosmic-comp today, and failure degrades to a glow without a hole
rather than a broken applet.
