# COSMIC Overlay Redo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the broken four-surface strip renderer with a single full-screen click-through layer surface drawn by a wgpu shader, and track the cursor through cosmic-comp's capture protocol instead of `/dev/input`.

**Architecture:** One `wlr-layer-shell` surface anchored to all four screen edges with an empty input region, rendered by a WGSL fragment shader that computes alpha from distance-to-nearest-edge with a soft cursor cutout. Cursor position comes from an `ext-image-copy-capture-v1` pointer cursor session running on its own Wayland connection in a background thread.

**Tech Stack:** Rust 2021, libcosmic (git `0bb006c`), iced shader widget over wgpu 27, `wayland-client` 0.31, `wayland-protocols` 0.32 (staging), `cosmic-config`, `bytemuck`.

**Spec:** `docs/superpowers/specs/2026-08-06-cosmic-overlay-redo-design.md`

## Global Constraints

- Branch: `desktop/cosmic`. Do not merge to `main`.
- `APP_ID` stays `com.github.twigglits.ringlight`.
- All wgpu types must come from `cosmic::iced_wgpu::wgpu` — never add a direct `wgpu` dependency, or the types will not match iced's (wgpu 27.0.1).
- `cosmic_config` is used via the `cosmic::cosmic_config` re-export — do not add a direct `cosmic-config` dependency.
- Every uniform length is in **physical** pixels. Proportional sizes are computed from the physical resolution inside `Primitive::prepare`, which is the only place `viewport.scale_factor()` is available.
- `MAX_ALPHA = 0.85`. The glow must never reach full opacity, even at brightness 1.0.
- No `process::exit`. No panics or `unwrap()` on Wayland, GPU, or config paths — every failure degrades and logs.
- Rust edition 2021, toolchain 1.94.0.

## Verification environment

This plan is written to be verified on the live session it targets: Pop!_OS 24.04, cosmic-comp, single output eDP-1 at 3000x2000 physical / 1500x1000 logical (200% scale).

Reinstall-and-restart after any change that needs visual checking:

```bash
cargo build --release \
  && sudo cp target/release/ringlight /usr/local/bin/ \
  && pkill -x cosmic-panel
```

**Restart the panel, not the applet.** cosmic-panel logs an applet's exit
(`ringlight: exited with code 137`) but does **not** respawn it. `cosmic-session`
does supervise cosmic-panel, so killing the panel brings it and every applet back.

**Verifying the overlay without clicking.** The glow auto-enables while a camera
is in use, so it can be triggered headlessly:

```bash
sleep 25 < /dev/video0 &        # hold the device open; auto-mode turns the glow on
sleep 5                          # the camera monitor polls every 2s
cosmic-screenshot --interactive=false --notify=false -s ./shots
```

Then sample pixels to confirm compositing exactly, rather than eyeballing it
(this machine has ImageMagick 6, so `convert`, not `magick`):

```bash
convert shot.png -format "%[pixel:p{1500,3}]" info:   # y=3 is behind the panel
```

To read logs, run the binary with logging enabled after killing the panel-spawned copy:

```bash
RUST_LOG=ringlight=debug,iced_renderer=warn journalctl --user -f -t ringlight
```

## File Structure

| File | Responsibility | Change |
|------|----------------|--------|
| `src/main.rs` | Entry point, module declarations | Modify |
| `src/app.rs` | `Application` impl: panel icon, popup, surface lifecycle, subscriptions | Rewrite in place |
| `src/settings.rs` | `RingLightSettings` + proportional derived values | Modify |
| `src/config.rs` | cosmic-config load/save | Create |
| `src/cursor.rs` | Capture-session cursor thread | Create |
| `src/glow/mod.rs` | `shader::Program` + `Primitive` + `Pipeline` | Create |
| `src/glow/glow.wgsl` | Vertex + fragment shader | Create |
| `src/camera.rs` | Camera detection | Unchanged |
| `src/mouse.rs` | `/dev/input` tracker | **Delete** (Task 5) |
| `src/overlay.rs` | Strip renderer | **Delete** (Task 1) |
| `examples/cursor_probe.rs` | Standalone protocol verification | Create (Task 8) |

---

### Task 1: Single full-screen click-through layer surface

Replaces four edge surfaces with one. Rendered as a flat translucent fill so the surface geometry can be verified without any shader risk.

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Delete: `src/overlay.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `RingLight.overlay_id: Option<Id>`; `RingLight::create_overlay(&mut self) -> Task<Message>`; `RingLight::destroy_overlay(&mut self) -> Task<Message>`; `RingLight::sync_overlay(&mut self) -> Task<Message>`; `RingLight::is_active(&self) -> bool`.

- [ ] **Step 1: Delete the strip renderer and its module declaration**

```bash
git rm src/overlay.rs
```

In `src/main.rs`, delete the line `mod overlay;`.

- [ ] **Step 2: Replace the overlay imports in `src/app.rs`**

Delete these two lines:

```rust
use crate::overlay::{self, EdgeSide, GlowCache, GlowProgram};
use cosmic::iced::{widget::canvas::Canvas, Alignment, Length, Limits, Subscription};
```

Replace with:

```rust
use cosmic::iced::{Alignment, Length, Limits, Subscription};
```

- [ ] **Step 3: Replace the four-surface state with one**

In `struct RingLight`, delete these two fields:

```rust
    /// Layer surface IDs: [top, bottom, left, right]
    overlay_ids: [Option<Id>; 4],
    glow_cache: GlowCache,
```

Replace with:

```rust
    overlay_id: Option<Id>,
```

Delete `screen_size` and `mouse_pos` too — neither is needed any more, because the shader derives everything from its own bounds:

```rust
    mouse_pos: (f64, f64),
    screen_size: (f32, f32),
```

In `init`, delete the matching initialisers (`mouse_pos`, `screen_size`, `overlay_ids`, `glow_cache`) and add:

```rust
            overlay_id: None,
```

- [ ] **Step 4: Replace surface creation with a single full-screen surface**

Replace the whole `create_overlay` and `destroy_overlay` pair with:

```rust
    fn create_overlay(&mut self) -> Task<Message> {
        if self.overlay_id.is_some() {
            return Task::none();
        }
        let id = Id::unique();
        self.overlay_id = Some(id);

        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: "ringlight".to_string(),
            // Top, not Overlay: the glow belongs above windows but below the
            // OSD and lock screen.
            layer: Layer::Top,
            // Anchored to both opposite edge pairs, so the compositor stretches
            // the surface across the whole output regardless of `size`.
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            size: Some((None, None)),
            // -1, not 0: 0 means "move me so I don't occlude the panel", which
            // pushes the glow inward off the true screen edge.
            exclusive_zone: -1,
            // Empty input region => no pointer input at all => click-through.
            input_zone: Some(Vec::new()),
            // The default max is 1920x1080, which would clamp larger screens.
            size_limits: Limits::NONE
                .min_width(1.0)
                .min_height(1.0)
                .max_width(16384.0)
                .max_height(16384.0),
            ..Default::default()
        })
    }

    fn destroy_overlay(&mut self) -> Task<Message> {
        match self.overlay_id.take() {
            Some(id) => destroy_layer_surface(id),
            None => Task::none(),
        }
    }
```

- [ ] **Step 5: Simplify the lifecycle helpers**

Replace `recreate_overlay` entirely — glow size no longer changes the surface geometry, only a uniform, so recreation is never needed. Delete this method:

```rust
    fn recreate_overlay(&mut self) -> Task<Message> { ... }
```

`sync_overlay` stays as-is. Then in `update`, replace both `return self.recreate_overlay();` calls (in `Message::SetGlowSize` and `Message::ApplyPreset`) with `Task::none()` by simply deleting the `return` lines, and delete every `self.glow_cache.clear_all();` line.

- [ ] **Step 6: Render a flat translucent fill**

Replace `view_window` and `overlay_view` with:

```rust
    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if self.popup == Some(id) {
            return self.popup_view();
        }
        if self.overlay_id == Some(id) {
            return self.overlay_view();
        }
        widget::text("").into()
    }
```

and, in the `impl RingLight` block:

```rust
    // Task 1 placeholder: a flat wash proves surface geometry, click-through
    // and lifecycle without involving the GPU pipeline. Task 2 replaces it.
    fn overlay_view(&self) -> Element<'_, Message> {
        widget::container(widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(|_theme| {
                cosmic::widget::container::Style {
                    background: Some(cosmic::iced::Background::Color(
                        cosmic::iced::Color::from_rgba(1.0, 0.78, 0.55, 0.25),
                    )),
                    ..Default::default()
                }
            }))
            .into()
    }
```

- [ ] **Step 7: Drop the now-dead mouse subscription**

In `subscription`, delete the entire `mouse_sub` binding and change the final line to:

```rust
        camera_sub
    }
```

Delete the `Message::MouseMoved(f64, f64)` variant and its `update` arm. Leave `src/mouse.rs` on disk for now; Task 5 deletes it. Remove `mod mouse;` from `src/main.rs` so the unused module does not warn.

- [ ] **Step 8: Build**

Run: `cargo build --release 2>&1 | tail -20`
Expected: compiles with zero errors and zero warnings.

- [ ] **Step 9: Install and verify on the live session**

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
```

Click the panel icon, toggle **Enabled** on, and confirm all four:

1. A warm translucent wash covers the **entire** screen.
2. It extends **behind the COSMIC panel**, all the way to the physical top edge — this is what `exclusive_zone: -1` bought.
3. Clicks pass through it: you can click a window, drag it, and use the panel normally.
4. Toggling **Enabled** off removes the wash completely.

If clicks are captured, the `input_zone` is not being applied — stop and report rather than proceeding.

- [ ] **Step 10: Commit**

```bash
git add -A src/ && git commit -m "feat: single full-screen click-through layer surface

Replaces four edge surfaces with one anchored to all edges. Fixes the
glow being pushed inward by the panel (exclusive_zone 0 -> -1) and
removes the per-edge coordinate mapping that made the cursor hole
misalign. Rendered as a flat wash for now; the shader lands next."
```

---

### Task 2: wgpu shader pipeline spike

**This is the risk gate.** It proves `shader::Program`/`Primitive`/`Pipeline` compile and render inside the applet process before any glow maths is written. If this task fails, stop and fall back to the Canvas approach documented in the spec's Risks section.

**Files:**
- Create: `src/glow/mod.rs`
- Create: `src/glow/glow.wgsl`
- Modify: `src/app.rs`, `src/main.rs`, `Cargo.toml`

**Interfaces:**
- Consumes: `RingLight::overlay_view`.
- Produces: `glow::GlowProgram { color: [f32; 3], brightness: f32 }`; `glow::GlowPrimitive`; `glow::GlowPipeline`.

- [ ] **Step 1: Add the bytemuck dependency**

In `Cargo.toml`, under `[dependencies]`, add:

```toml
bytemuck = { version = "1", features = ["derive"] }
```

`bytemuck` 1.25.0 is already in `Cargo.lock` as a transitive dependency, so this resolves without a new download.

- [ ] **Step 2: Write the flat-colour shader**

Create `src/glow/glow.wgsl`:

```wgsl
struct Uniforms {
    resolution: vec2<f32>,
    cursor: vec2<f32>,
    color: vec3<f32>,
    brightness: f32,
    glow_width: f32,
    hole_radius: f32,
    hole_softness: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// Full-screen triangle: three vertices, no vertex buffer.
//   i=0 -> (-1,-1)   i=1 -> (3,-1)   i=2 -> (-1,3)
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

// Task 2 spike: flat translucent fill. Task 4 replaces this body.
@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let a = u.brightness * 0.25;
    return vec4<f32>(u.color * a, a);   // premultiplied
}
```

- [ ] **Step 3: Write the pipeline, primitive and program**

Create `src/glow/mod.rs`:

```rust
//! GPU glow renderer.
//!
//! One full-screen triangle, one fragment shader. All lengths in the uniform
//! block are PHYSICAL pixels: `Program::draw` only sees logical bounds, so the
//! conversion happens in `Primitive::prepare`, which is the only place the
//! viewport scale factor is available.

use cosmic::iced::{mouse, Rectangle};
use cosmic::iced_wgpu::wgpu;
use cosmic::iced_widget::shader::{self, Viewport};

/// Peak opacity at brightness 1.0. Caps the glow so it always reads as light
/// rather than paint -- the reason the old renderer produced opaque bars.
const MAX_ALPHA: f32 = 0.85;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    cursor: [f32; 2],
    color: [f32; 3],
    brightness: f32,
    glow_width: f32,
    hole_radius: f32,
    hole_softness: f32,
    _pad: f32,
}

pub struct GlowPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl shader::Pipeline for GlowPipeline {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ringlight glow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("glow.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ringlight glow uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ringlight glow bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ringlight glow bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ringlight glow pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ringlight glow pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // The shader outputs premultiplied colour, matching how
                    // iced composites its own primitives.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self { pipeline, uniform_buffer, bind_group }
    }
}

#[derive(Debug)]
pub struct GlowPrimitive {
    color: [f32; 3],
    brightness: f32,
    /// Fraction of the smaller screen dimension.
    glow_fraction: f32,
    /// Fraction of the smaller screen dimension. 0.0 disables the hole.
    hole_fraction: f32,
    /// Normalized [0,1] cursor position.
    cursor: [f32; 2],
    hole_softness: f32,
}

impl shader::Primitive for GlowPrimitive {
    type Pipeline = GlowPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // bounds is logical; everything below is physical.
        let scale = viewport.scale_factor() as f32;
        let resolution = [bounds.width * scale, bounds.height * scale];
        let min_dim = resolution[0].min(resolution[1]);

        let uniforms = Uniforms {
            resolution,
            cursor: self.cursor,
            color: self.color,
            brightness: self.brightness,
            glow_width: self.glow_fraction * min_dim,
            hole_radius: self.hole_fraction * min_dim,
            hole_softness: self.hole_softness,
            _pad: 0.0,
        };

        queue.write_buffer(&pipeline.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        true
    }
}

pub struct GlowProgram {
    pub color: [f32; 3],
    pub brightness: f32,
    pub glow_fraction: f32,
    pub hole_fraction: f32,
    pub cursor: [f32; 2],
}

impl<Message> shader::Program<Message> for GlowProgram {
    type State = ();
    type Primitive = GlowPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        GlowPrimitive {
            color: self.color,
            brightness: self.brightness * MAX_ALPHA,
            glow_fraction: self.glow_fraction,
            hole_fraction: self.hole_fraction,
            cursor: self.cursor,
            hole_softness: 0.55,
        }
    }
}
```

- [ ] **Step 4: Wire the shader widget into the overlay view**

In `src/main.rs`, add `mod glow;`.

In `src/app.rs`, add to the imports:

```rust
use cosmic::iced_widget::shader::Shader;
```

Replace the Task 1 placeholder `overlay_view` with:

```rust
    fn overlay_view(&self) -> Element<'_, Message> {
        Shader::new(GlowProgram {
            color: self.settings.glow_color(),
            brightness: self.settings.brightness,
            glow_fraction: 0.10,
            hole_fraction: 0.0,
            cursor: [0.5, 0.5],
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
```

and add `use crate::glow::GlowProgram;` to the imports.

- [ ] **Step 5: Build**

Run: `cargo build --release 2>&1 | tail -30`
Expected: compiles clean.

If it fails because `cosmic::iced_widget::shader` does not exist, the `wgpu` feature is not propagating to `iced_widget`. Add `"iced_widget/wgpu"` alongside the existing features in `Cargo.toml`'s libcosmic dependency and rebuild. If it still fails, **stop** — this is the fallback trigger described in the spec.

- [ ] **Step 6: Install and verify the GPU path is live**

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
```

Toggle on. Expected: a flat translucent warm wash, visually the same as Task 1 but now drawn by the GPU.

Then confirm the wgpu renderer — not the tiny-skia fallback — is active:

```bash
journalctl --user -t ringlight --since "2 min ago" | grep -i "not supported with this renderer"
```

Expected: **no output**. If that warning appears, iced fell back to tiny-skia, custom primitives are silently discarded, and nothing will ever render. Stop and report.

- [ ] **Step 7: Commit**

```bash
git add -A src/ Cargo.toml Cargo.lock
git commit -m "feat: wgpu shader pipeline for the glow overlay

Spike proving shader::Program/Primitive/Pipeline render inside the applet
process. Flat fill for now; edge falloff lands next. All uniform lengths
are physical pixels, converted in prepare() where the scale factor lives."
```

---

### Task 3: Proportional glow sizing

Pure functions, fully unit-tested off-GPU. Fixed pixel widths are why the glow reads as far too thick: 180px is 18% of a 1000px logical height.

**Files:**
- Modify: `src/settings.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `RingLightSettings::glow_fraction(&self) -> f32`; `RingLightSettings::hole_fraction(&self) -> f32`; `settings::scale_to_min_dimension(fraction: f32, resolution: [f32; 2]) -> f32`.

- [ ] **Step 1: Write the failing tests**

Append to `src/settings.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_fractions_are_ordered_and_sane() {
        let small = RingLightSettings { glow_size: GlowSize::Small, ..Default::default() };
        let medium = RingLightSettings { glow_size: GlowSize::Medium, ..Default::default() };
        let large = RingLightSettings { glow_size: GlowSize::Large, ..Default::default() };

        assert!(small.glow_fraction() < medium.glow_fraction());
        assert!(medium.glow_fraction() < large.glow_fraction());
        // A glow wider than a quarter of the screen stops being a glow.
        assert!(large.glow_fraction() < 0.25);
        assert!(small.glow_fraction() > 0.0);
    }

    #[test]
    fn hole_off_is_zero() {
        let off = RingLightSettings { hole_size: HoleSize::Off, ..Default::default() };
        assert_eq!(off.hole_fraction(), 0.0);
    }

    #[test]
    fn scaling_uses_the_smaller_dimension() {
        // Landscape: height is smaller, so it governs.
        assert_eq!(scale_to_min_dimension(0.10, [3000.0, 2000.0]), 200.0);
        // Portrait: width governs.
        assert_eq!(scale_to_min_dimension(0.10, [2000.0, 3000.0]), 200.0);
        // Square.
        assert_eq!(scale_to_min_dimension(0.5, [1000.0, 1000.0]), 500.0);
    }

    #[test]
    fn medium_glow_matches_the_spec_table() {
        let s = RingLightSettings { glow_size: GlowSize::Medium, ..Default::default() };
        // 3000x2000 physical at 200% => 200 physical px => 100 logical px.
        assert_eq!(scale_to_min_dimension(s.glow_fraction(), [3000.0, 2000.0]), 200.0);
    }

    #[test]
    fn glow_color_endpoints_are_warm_and_cool() {
        let warm = RingLightSettings { color_temp: 0.0, ..Default::default() };
        let cool = RingLightSettings { color_temp: 1.0, ..Default::default() };
        // Warm has more red than blue; cool has more blue than warm.
        assert!(warm.glow_color()[0] > warm.glow_color()[2]);
        assert!(cool.glow_color()[2] > warm.glow_color()[2]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib 2>&1 | tail -20`

If the crate has no lib target, run `cargo test --bins` instead and use that form for the rest of this plan.

Expected: FAIL — `no method named glow_fraction`, `cannot find function scale_to_min_dimension`.

- [ ] **Step 3: Implement the proportional sizing**

In `src/settings.rs`, delete `glow_width()` and `hole_radius()` entirely and replace with:

```rust
    /// Glow depth as a fraction of the smaller screen dimension.
    ///
    /// Proportional rather than a fixed pixel count, so the glow looks the
    /// same on any display. The old fixed 180px was 18% of a 1000px logical
    /// height, which is why it read as far too thick.
    pub fn glow_fraction(&self) -> f32 {
        match self.glow_size {
            GlowSize::Small => 0.06,
            GlowSize::Medium => 0.10,
            GlowSize::Large => 0.16,
        }
    }

    /// Cursor hole radius as a fraction of the smaller screen dimension.
    pub fn hole_fraction(&self) -> f32 {
        match self.hole_size {
            HoleSize::Off => 0.0,
            HoleSize::Small => 0.08,
            HoleSize::Medium => 0.14,
            HoleSize::Large => 0.22,
        }
    }
```

Then add at module level, outside the `impl`:

```rust
/// Convert a fraction of the smaller screen dimension into pixels.
///
/// `resolution` must be in physical pixels, so the result is too.
pub fn scale_to_min_dimension(fraction: f32, resolution: [f32; 2]) -> f32 {
    fraction * resolution[0].min(resolution[1])
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib 2>&1 | tail -20`
Expected: PASS, 5 tests.

- [ ] **Step 5: Make the renderer use the tested function**

Task 2's `prepare` computes the same multiplication inline. Route it through the
function that now has tests, so the tested path is the shipping path.

In `src/glow/mod.rs`, add to the imports:

```rust
use crate::settings::scale_to_min_dimension;
```

and in `Primitive::prepare`, replace these three lines:

```rust
        let min_dim = resolution[0].min(resolution[1]);
```

...and the two uniform fields:

```rust
            glow_width: self.glow_fraction * min_dim,
            hole_radius: self.hole_fraction * min_dim,
```

with:

```rust
            glow_width: scale_to_min_dimension(self.glow_fraction, resolution),
            hole_radius: scale_to_min_dimension(self.hole_fraction, resolution),
```

deleting the now-unused `min_dim` binding.

- [ ] **Step 6: Build and confirm nothing else referenced the old methods**

Run: `cargo build --release 2>&1 | tail -20`
Expected: clean. `glow_width()` and `hole_radius()` had no remaining callers after Task 1 deleted `overlay.rs`.

- [ ] **Step 7: Commit**

```bash
git add -A src/
git commit -m "feat: proportional glow and hole sizing

Glow depth becomes a fraction of the smaller screen dimension instead of
a fixed pixel count, so it looks identical at any resolution or scale.
180px was 18% of this display's logical height."
```

---

### Task 4: Real glow shader maths

Replaces the flat fill with edge falloff. This is the task that retires the "solid opaque bars" and "hard inner line" symptoms.

**Files:**
- Modify: `src/glow/glow.wgsl`, `src/app.rs`

**Interfaces:**
- Consumes: `RingLightSettings::glow_fraction`, `glow_color`.
- Produces: no new signatures.

- [ ] **Step 1: Replace the fragment shader body**

In `src/glow/glow.wgsl`, replace the whole `fs_main` function with:

```wgsl
@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = pos.xy;
    let size = u.resolution;

    // Distance to the NEAREST edge. Taking the minimum over all four edges
    // means corners need no special case and no seams can exist -- the whole
    // reason this is one surface rather than four.
    let d = min(min(p.x, size.x - p.x), min(p.y, size.y - p.y));

    // Quadratic falloff reaching exactly zero at glow_width. A single alpha
    // term, not five summed: summing was what saturated the old renderer to
    // fully opaque bars.
    let t = d / max(u.glow_width, 1.0);
    var a = pow(max(0.0, 1.0 - t), 2.0);
    a = a * u.brightness;

    if (a <= 0.0) {
        discard;
    }

    return vec4<f32>(u.color * a, a);   // premultiplied
}
```

- [ ] **Step 2: Feed the real settings into the program**

In `src/app.rs`, update `overlay_view` to use the settings rather than the hardcoded fraction:

```rust
    fn overlay_view(&self) -> Element<'_, Message> {
        Shader::new(GlowProgram {
            color: self.settings.glow_color(),
            brightness: self.settings.brightness,
            glow_fraction: self.settings.glow_fraction(),
            hole_fraction: 0.0, // Task 6 wires up the cursor hole
            cursor: [0.5, 0.5],
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
```

- [ ] **Step 3: Build**

Run: `cargo build --release 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 4: Install and verify the glow visually**

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
```

Toggle on and check all six:

1. A **soft** glow hugs all four edges and fades smoothly inward.
2. **No hard line** anywhere — the old clipped seam is gone.
3. **No seams at the corners**; the glow wraps continuously around them.
4. At brightness 1.0 you can still **read text underneath** the brightest part. If it looks like paint, `MAX_ALPHA` is not being applied.
5. **S / M / L** visibly change the glow depth, and Medium is roughly a tenth of the screen height.
6. The **warm/cool slider** shifts the colour from amber to a blue-white.

- [ ] **Step 5: Commit**

```bash
git add -A src/
git commit -m "feat: edge-falloff glow shader

Alpha is a single quadratic falloff on distance-to-nearest-edge, capped
at MAX_ALPHA. Replaces five summed passes that saturated to opaque, and
40-strip gradient approximation that banded. min() over four edges makes
corners and seams structurally impossible."
```

---

### Task 5: Cursor tracking via the capture protocol

Replaces `/dev/input`, which cannot work here — the user is not in the `input` group — and which would drift even with permission because it accumulates raw relative deltas without pointer acceleration.

**Files:**
- Create: `src/cursor.rs`
- Modify: `src/main.rs`, `Cargo.toml`
- Delete: `src/mouse.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `cursor::CursorState { pos: [f32; 2], visible: bool }` (Debug + Clone + Copy + PartialEq); `cursor::start() -> tokio::sync::watch::Receiver<CursorState>`; `cursor::normalize(x: i32, y: i32, buffer: (i32, i32), swap_axes: bool) -> [f32; 2]`.

- [ ] **Step 1: Add the Wayland dependencies**

In `Cargo.toml` under `[dependencies]`:

```toml
wayland-client = "0.31"
wayland-protocols = { version = "0.32", features = ["client", "staging"] }
```

Both are already in `Cargo.lock` as transitive dependencies at compatible versions (0.31.13 and 0.32.11).

- [ ] **Step 2: Write the failing test for coordinate normalization**

Create `src/cursor.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_against_the_buffer_size() {
        // Centre of a 3000x2000 output.
        assert_eq!(normalize(1500, 1000, (3000, 2000), false), [0.5, 0.5]);
        // Origin and far corner.
        assert_eq!(normalize(0, 0, (3000, 2000), false), [0.0, 0.0]);
        assert_eq!(normalize(3000, 2000, (3000, 2000), false), [1.0, 1.0]);
    }

    #[test]
    fn swaps_axes_for_rotated_outputs() {
        // A 90-degree transform means the buffer's logical extent is swapped.
        assert_eq!(normalize(1000, 1500, (3000, 2000), true), [0.5, 0.5]);
    }

    #[test]
    fn clamps_out_of_range_positions() {
        assert_eq!(normalize(-50, -50, (3000, 2000), false), [0.0, 0.0]);
        assert_eq!(normalize(9999, 9999, (3000, 2000), false), [1.0, 1.0]);
    }

    #[test]
    fn degenerate_buffer_does_not_divide_by_zero() {
        let p = normalize(10, 10, (0, 0), false);
        assert!(p[0].is_finite() && p[1].is_finite());
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Add `mod cursor;` to `src/main.rs`, then run: `cargo test --lib cursor 2>&1 | tail -20`
Expected: FAIL — `cannot find function normalize`.

- [ ] **Step 4: Implement the normalization function**

Prepend to `src/cursor.rs`:

```rust
//! Global cursor position via cosmic-comp's ext-image-copy-capture-v1.
//!
//! The pointer cursor session reports position independently of frame
//! capture, so this needs no permissions, never grabs input, attaches no
//! buffers, and captures no frames. It replaces the /dev/input reader, which
//! needed `input` group membership and drifted because it accumulated raw
//! relative deltas with no pointer acceleration applied.
//!
//! Positions are published NORMALIZED to [0,1] so nothing downstream has to
//! know the output scale -- which is what made the old code wrong on this
//! 200%-scaled display.

use std::time::Duration;
use tokio::sync::watch;

/// Convert a compositor cursor position into a normalized [0,1] coordinate.
///
/// `buffer` is the output's physical size; `swap_axes` is true for 90/270
/// degree transforms, where the buffer's extents are exchanged.
pub fn normalize(x: i32, y: i32, buffer: (i32, i32), swap_axes: bool) -> [f32; 2] {
    let (w, h) = if swap_axes { (buffer.1, buffer.0) } else { buffer };
    if w <= 0 || h <= 0 {
        return [0.5, 0.5];
    }
    [
        (x as f32 / w as f32).clamp(0.0, 1.0),
        (y as f32 / h as f32).clamp(0.0, 1.0),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorState {
    /// Normalized [0,1] position within the output.
    pub pos: [f32; 2],
    /// False when the cursor has left the output or tracking is unavailable.
    pub visible: bool,
}

impl Default for CursorState {
    fn default() -> Self {
        Self { pos: [0.5, 0.5], visible: false }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib cursor 2>&1 | tail -20`
Expected: PASS, 4 tests.

- [ ] **Step 6: Implement the Wayland session thread**

Append to `src/cursor.rs`:

```rust
use wayland_client::{
    protocol::{
        wl_output::{self, WlOutput},
        wl_pointer::WlPointer,
        wl_registry,
        wl_seat::{self, WlSeat},
    },
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_cursor_session_v1::{self, ExtImageCopyCaptureCursorSessionV1},
    ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
};

enum SessionError {
    /// The compositor does not offer what we need. Retrying will not help.
    Unsupported(&'static str),
    /// The connection died. Retrying may help.
    Transient(String),
}

struct State {
    tx: watch::Sender<CursorState>,
    output: Option<WlOutput>,
    seat: Option<WlSeat>,
    has_pointer: bool,
    source_mgr: Option<ExtOutputImageCaptureSourceManagerV1>,
    capture_mgr: Option<ExtImageCopyCaptureManagerV1>,
    buffer: Option<(i32, i32)>,
    swap_axes: bool,
    visible: bool,
}

impl State {
    fn publish(&self, pos: [f32; 2]) {
        let _ = self.tx.send(CursorState { pos, visible: self.visible });
    }
}

/// Start cursor tracking. Returns immediately; the receiver holds
/// `CursorState::default()` until the first position arrives.
pub fn start() -> watch::Receiver<CursorState> {
    let (tx, rx) = watch::channel(CursorState::default());

    std::thread::spawn(move || {
        let mut backoff = Duration::from_millis(250);
        loop {
            match run_session(&tx) {
                Err(SessionError::Unsupported(what)) => {
                    log::warn!(
                        "ringlight: cursor tracking unavailable ({what}); \
                         the glow will render without a cursor hole"
                    );
                    let _ = tx.send(CursorState { pos: [0.5, 0.5], visible: false });
                    return; // Will not become available later.
                }
                Err(SessionError::Transient(e)) => {
                    log::warn!("ringlight: cursor session lost: {e}; retrying");
                }
                Ok(()) => {
                    log::warn!("ringlight: cursor session ended; retrying");
                }
            }

            if tx.send(CursorState { pos: [0.5, 0.5], visible: false }).is_err() {
                return; // Application gone.
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });

    rx
}

fn run_session(tx: &watch::Sender<CursorState>) -> Result<(), SessionError> {
    // Our own connection, deliberately separate from the one iced-sctk owns.
    let conn = Connection::connect_to_env()
        .map_err(|e| SessionError::Transient(format!("connect failed: {e}")))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());

    let mut state = State {
        tx: tx.clone(),
        output: None,
        seat: None,
        has_pointer: false,
        source_mgr: None,
        capture_mgr: None,
        buffer: None,
        swap_axes: false,
        visible: false,
    };

    // First roundtrip binds globals; the second delivers seat capabilities
    // and the output mode.
    queue.roundtrip(&mut state).map_err(|e| SessionError::Transient(e.to_string()))?;
    queue.roundtrip(&mut state).map_err(|e| SessionError::Transient(e.to_string()))?;

    let source_mgr = state.source_mgr.clone()
        .ok_or(SessionError::Unsupported("no ext_output_image_capture_source_manager_v1"))?;
    let capture_mgr = state.capture_mgr.clone()
        .ok_or(SessionError::Unsupported("no ext_image_copy_capture_manager_v1"))?;
    let output = state.output.clone()
        .ok_or(SessionError::Unsupported("no wl_output"))?;
    let seat = state.seat.clone()
        .ok_or(SessionError::Unsupported("no wl_seat"))?;
    if !state.has_pointer {
        return Err(SessionError::Unsupported("seat has no pointer"));
    }

    let pointer = seat.get_pointer(&qh, ());
    let source = source_mgr.create_source(&output, &qh, ());
    // No buffer is ever attached and no frame is ever captured: position
    // events are independent of the capture pipeline.
    let _session = capture_mgr.create_pointer_cursor_session(&source, &pointer, &qh, ());

    queue.roundtrip(&mut state).map_err(|e| SessionError::Transient(e.to_string()))?;
    log::info!("ringlight: cursor tracking active via ext-image-copy-capture");

    loop {
        queue
            .blocking_dispatch(&mut state)
            .map_err(|e| SessionError::Transient(e.to_string()))?;
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global { name, interface, version } = event else {
            return;
        };
        match interface.as_str() {
            "wl_output" if state.output.is_none() => {
                state.output = Some(registry.bind(name, version.min(4), qh, ()));
            }
            "wl_seat" if state.seat.is_none() => {
                state.seat = Some(registry.bind(name, version.min(7), qh, ()));
            }
            "ext_output_image_capture_source_manager_v1" => {
                state.source_mgr = Some(registry.bind(name, 1, qh, ()));
            }
            "ext_image_copy_capture_manager_v1" => {
                state.capture_mgr = Some(registry.bind(name, 1, qh, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event {
            state.has_pointer = caps.contains(wl_seat::Capability::Pointer);
        }
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Mode { flags, width, height, .. } => {
                let current = matches!(flags, WEnum::Value(f) if f.contains(wl_output::Mode::Current));
                if current {
                    state.buffer = Some((width, height));
                }
            }
            wl_output::Event::Geometry { transform, .. } => {
                state.swap_axes = matches!(
                    transform,
                    WEnum::Value(wl_output::Transform::_90)
                        | WEnum::Value(wl_output::Transform::_270)
                        | WEnum::Value(wl_output::Transform::Flipped90)
                        | WEnum::Value(wl_output::Transform::Flipped270)
                );
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureCursorSessionV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureCursorSessionV1,
        event: ext_image_copy_capture_cursor_session_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_image_copy_capture_cursor_session_v1::Event;
        match event {
            Event::Enter => state.visible = true,
            Event::Leave => {
                // Suppress the hole rather than strand it at a stale position.
                state.visible = false;
                let pos = state.tx.borrow().pos;
                state.publish(pos);
            }
            Event::Position { x, y } => {
                let Some(buffer) = state.buffer else { return };
                state.publish(normalize(x, y, buffer, state.swap_axes));
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(State: ignore WlPointer);
wayland_client::delegate_noop!(State: ExtOutputImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(State: ExtImageCopyCaptureManagerV1);
wayland_client::delegate_noop!(State: ignore ExtImageCaptureSourceV1);
```

- [ ] **Step 7: Delete the /dev/input tracker**

```bash
git rm src/mouse.rs
```

`mod mouse;` was already removed from `src/main.rs` in Task 1.

- [ ] **Step 8: Run all tests and build**

Run: `cargo test --lib 2>&1 | tail -20`
Expected: PASS, 9 tests.

Run: `cargo build --release 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 9: Verify tracking is live**

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
sleep 2
journalctl --user -t ringlight --since "1 min ago" | grep -i cursor
```

Expected: `cursor tracking active via ext-image-copy-capture`.

If instead you see `cursor tracking unavailable`, the globals were not bound; re-run `cargo run --example cursor_probe` once Task 8 adds it, or check `strings /usr/bin/cosmic-comp | grep ext_image_copy_capture`.

- [ ] **Step 10: Commit**

```bash
git add -A src/ Cargo.toml Cargo.lock
git commit -m "feat: cursor tracking via ext-image-copy-capture

Replaces the /dev/input reader, which never ran (user not in the input
group) and would have drifted anyway. The capture protocol's pointer
cursor session gives true compositor coordinates with no permissions, no
input grab, and no frames captured. Positions are normalized so nothing
downstream needs to know the output scale."
```

---

### Task 6: Cursor hole

Wires cursor state into the shader through a rate-limited subscription. The rate limit matters: the old code sent one message per input event and repainted four surfaces each time.

**Files:**
- Modify: `src/app.rs`, `src/glow/glow.wgsl`

**Interfaces:**
- Consumes: `cursor::start`, `cursor::CursorState`, `RingLightSettings::hole_fraction`.
- Produces: `Message::CursorMoved(CursorState)`.

- [ ] **Step 1: Add the hole to the fragment shader**

In `src/glow/glow.wgsl`, insert immediately before the `if (a <= 0.0)` guard in `fs_main`:

```wgsl
    // Soft circular cutout so the glow does not wash out whatever the cursor
    // is pointing at. hole_radius of 0 disables it entirely.
    if (u.hole_radius > 0.0) {
        let dist = distance(p, u.cursor * size);
        a = a * smoothstep(u.hole_radius * u.hole_softness, u.hole_radius, dist);
    }
```

- [ ] **Step 2: Add cursor state to the application**

In `src/app.rs`, add to `struct RingLight`:

```rust
    cursor: crate::cursor::CursorState,
```

and to `init`:

```rust
            cursor: crate::cursor::CursorState::default(),
```

Add the message variant:

```rust
    CursorMoved(crate::cursor::CursorState),
```

and the `update` arm:

```rust
            Message::CursorMoved(c) => {
                self.cursor = c;
            }
```

- [ ] **Step 3: Add the rate-limited subscription**

In `src/app.rs`, add these imports:

```rust
use std::time::{Duration, Instant};
```

In `subscription`, add before the final line:

```rust
        // The watch channel coalesces, and this throttles further: at most one
        // message per frame, and nothing at all for sub-pixel movement. The
        // old code emitted one message per raw input event and repainted four
        // surfaces for each.
        let cursor_sub = Subscription::run(|| {
            async {
                let rx = crate::cursor::start();
                let init = *rx.borrow();
                futures_util::stream::unfold(
                    (rx, Instant::now(), init),
                    |(mut rx, mut last_emit, mut last)| async move {
                        loop {
                            rx.changed().await.ok()?;
                            let now = *rx.borrow();

                            let moved = (now.pos[0] - last.pos[0]).abs()
                                + (now.pos[1] - last.pos[1]).abs()
                                > 0.0005; // ~1.5px on a 3000px-wide output
                            let toggled = now.visible != last.visible;

                            if toggled
                                || (moved
                                    && last_emit.elapsed() >= Duration::from_millis(16))
                            {
                                last_emit = Instant::now();
                                last = now;
                                return Some((
                                    Message::CursorMoved(now),
                                    (rx, last_emit, last),
                                ));
                            }
                        }
                    },
                )
            }
            .flatten_stream()
        });

        Subscription::batch(vec![camera_sub, cursor_sub])
    }
```

Delete the now-obsolete bare `camera_sub` line that Task 1 left as the function's tail expression.

- [ ] **Step 4: Feed the hole into the shader program**

In `overlay_view`, replace the two placeholder lines:

```rust
            hole_fraction: if self.cursor.visible {
                self.settings.hole_fraction()
            } else {
                0.0
            },
            cursor: self.cursor.pos,
```

- [ ] **Step 5: Build and test**

Run: `cargo test --lib 2>&1 | tail -10`
Expected: PASS, 9 tests.

Run: `cargo build --release 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 6: Install and verify the hole**

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
```

Toggle on, set Cursor Hole to **M**, and check all five:

1. Moving the cursor into the glow opens a **soft-edged** gap that follows it.
2. The gap is **centred on the cursor** — not offset. Check near **all four edges** and in a **corner**; the old bug misaligned it by 80px vertically and 420px horizontally.
3. Motion is **smooth**, with no visible lag or stutter.
4. **Off** removes the hole entirely; **S / L** visibly change its size.
5. `top -p "$(pgrep -x ringlight)"` while waving the cursor stays in single-digit CPU percent.

- [ ] **Step 7: Commit**

```bash
git add -A src/
git commit -m "feat: cursor hole driven by compositor cursor position

Soft smoothstep cutout in the shader, fed by a rate-limited subscription
that emits at most one message per frame and ignores sub-pixel movement."
```

---

### Task 7: Settings persistence

Settings currently reset whenever cosmic-panel respawns the applet.

**Files:**
- Create: `src/config.rs`
- Modify: `src/settings.rs`, `src/app.rs`, `src/main.rs`

**Interfaces:**
- Consumes: `RingLightSettings`.
- Produces: `config::load() -> RingLightSettings`; `config::save(&RingLightSettings)`; `Message::PersistSettings`.

- [ ] **Step 1: Write the failing test**

Create `src/config.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use crate::settings::{GlowSize, HoleSize, RingLightSettings};

    #[test]
    fn settings_round_trip_through_json() {
        let original = RingLightSettings {
            enabled: true,
            brightness: 0.42,
            color_temp: 0.9,
            auto_mode: false,
            glow_size: GlowSize::Large,
            hole_size: HoleSize::Off,
        };

        let encoded = serde_json::to_string(&original).expect("serialize");
        let decoded: RingLightSettings =
            serde_json::from_str(&encoded).expect("deserialize");

        assert_eq!(decoded.enabled, original.enabled);
        assert_eq!(decoded.brightness, original.brightness);
        assert_eq!(decoded.color_temp, original.color_temp);
        assert_eq!(decoded.auto_mode, original.auto_mode);
        assert_eq!(decoded.glow_size, original.glow_size);
        assert_eq!(decoded.hole_size, original.hole_size);
    }
}
```

Add `serde_json = "1"` to `[dev-dependencies]` in `Cargo.toml` (create the section if absent), and `mod config;` to `src/main.rs`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib config 2>&1 | tail -20`
Expected: FAIL — `RingLightSettings: PartialEq` is not satisfied for the field comparisons, or the derive is missing.

- [ ] **Step 3: Make the settings type persistable**

In `src/settings.rs`, change the derive on `RingLightSettings` to add `PartialEq` and the config entry:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry)]
#[version = 1]
pub struct RingLightSettings {
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib config 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Implement load and save**

Prepend to `src/config.rs`:

```rust
//! Settings persistence via cosmic-config.
//!
//! Every failure degrades to in-memory defaults: losing saved brightness is
//! never a reason to take down a panel applet.

use crate::settings::RingLightSettings;
use cosmic::cosmic_config::{Config, CosmicConfigEntry};

const APP_ID: &str = "com.github.twigglits.ringlight";

fn config() -> Option<Config> {
    match Config::new(APP_ID, RingLightSettings::VERSION) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("ringlight: cosmic-config unavailable: {e}");
            None
        }
    }
}

/// Load persisted settings, falling back to defaults.
pub fn load() -> RingLightSettings {
    let Some(cfg) = config() else {
        return RingLightSettings::default();
    };
    match RingLightSettings::get_entry(&cfg) {
        Ok(s) => s,
        Err((errors, partial)) => {
            for e in errors {
                log::warn!("ringlight: config key unreadable, using default: {e}");
            }
            partial
        }
    }
}

/// Persist settings. Errors are logged, never propagated.
pub fn save(settings: &RingLightSettings) {
    let Some(cfg) = config() else { return };
    if let Err(e) = settings.write_entry(&cfg) {
        log::warn!("ringlight: could not save settings: {e}");
    }
}
```

- [ ] **Step 6: Load at startup and save on change**

In `src/app.rs`, change the `init` settings initialiser from `RingLightSettings::default()` to:

```rust
            settings: crate::config::load(),
```

Add the message variant:

```rust
    PersistSettings,
```

and the `update` arm:

```rust
            Message::PersistSettings => {
                crate::config::save(&self.settings);
            }
```

Then persist after each discrete change. Append `crate::config::save(&self.settings);` to the end of the `ToggleEnabled`, `ToggleAutoMode`, `SetGlowSize`, `SetHoleSize` and `ApplyPreset` arms — but **not** `SetBrightness` or `SetColorTemp`, which fire continuously while a slider is dragged.

For those two, persist on release instead. In `popup_view`, add `.on_release(Message::PersistSettings)` to both sliders:

```rust
            .push(
                widget::slider(0.0..=1.0, self.settings.brightness, Message::SetBrightness)
                    .step(0.05)
                    .on_release(Message::PersistSettings),
            )
```

and the same for the `SetColorTemp` slider.

- [ ] **Step 7: Build and test**

Run: `cargo test --lib 2>&1 | tail -10`
Expected: PASS, 10 tests.

Run: `cargo build --release 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 8: Verify persistence survives a restart**

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
```

1. Set brightness to a distinctive value, choose glow size **L** and cursor hole **S**.
2. `pkill -x cosmic-panel` and wait for cosmic-panel to respawn it.
3. Open the popup: brightness, glow size and hole size must all be as you left them.
4. Confirm the file exists: `ls ~/.config/cosmic/com.github.twigglits.ringlight/`

- [ ] **Step 9: Commit**

```bash
git add -A src/ Cargo.toml Cargo.lock
git commit -m "feat: persist settings via cosmic-config

Discrete controls save immediately; sliders save on release so dragging
does not write once per frame. Every config failure degrades to in-memory
defaults."
```

---

### Task 8: Cleanup, probe, and documentation

**Files:**
- Create: `examples/cursor_probe.rs`
- Modify: `src/app.rs`, `CLAUDE.md`, `README.md`, `Cargo.toml`

**Interfaces:**
- Consumes: everything above.
- Produces: nothing.

- [ ] **Step 1: Remove the Quit control**

A panel applet's lifetime belongs to cosmic-panel; `process::exit(0)` leaves a dead icon in the panel.

In `src/app.rs`, delete the `Message::Quit` variant, its `update` arm (containing `std::process::exit(0)`), and this line from `popup_view`:

```rust
            .push(widget::button::text("Quit").on_press(Message::Quit));
```

Change the preceding `.push(widget::divider::horizontal::default())` to end the chain with a `;`.

- [ ] **Step 2: Add the protocol probe as an example**

Copy the verified probe into the repository:

```bash
mkdir -p examples
cp /tmp/claude-1000/-home-jeannaude-Documents-ringlight/e34e53a0-a770-4c22-b415-61c660ef3393/scratchpad/cursor-probe/src/main.rs \
   examples/cursor_probe.rs
```

If that scratchpad path no longer exists, the probe's full source is reproduced in the spec's research section; recreate it from there.

Verify it still builds and runs against the compositor:

```bash
cargo run --release --example cursor_probe 2>&1 | tail -20
```

Expected: both globals `PRESENT`, session created, position events streaming.

- [ ] **Step 3: Run the full check**

```bash
cargo test --lib 2>&1 | tail -10
cargo build --release 2>&1 | tail -20
cargo clippy --release --all-targets 2>&1 | grep -E "^(warning|error)" | head -20
```

Expected: 10 tests pass, build clean, no clippy warnings. Fix any that appear.

- [ ] **Step 4: Update CLAUDE.md**

Replace the **Architecture**, **Key design decisions** and **Status** sections to match reality:

```markdown
## Architecture

```
src/
├── main.rs       Entry point: cosmic::applet::run
├── app.rs        COSMIC Application trait impl (panel icon, popup, overlay lifecycle)
├── glow/         GPU glow renderer (iced shader widget over wgpu)
│   ├── mod.rs    shader::Program + Primitive + Pipeline
│   └── glow.wgsl Full-screen triangle + edge-falloff fragment shader
├── cursor.rs     Global cursor position via ext-image-copy-capture-v1
├── camera.rs     Async /proc/*/fd/ scanner for webcam detection
├── config.rs     Settings persistence via cosmic-config
└── settings.rs   RingLightSettings (brightness, color_temp, proportional sizes)
```

**Overlay approach**: A single layer-shell surface anchored to all four edges,
with `input_zone: Some(Vec::new())` (empty Wayland input region) so it is fully
click-through, and `exclusive_zone: -1` so it reaches the true screen edge rather
than being pushed inward by the panel.

**Glow rendering**: A WGSL fragment shader computes alpha from distance to the
nearest edge with a quadratic falloff capped at `MAX_ALPHA = 0.85`, plus a
`smoothstep` cutout at the cursor. Taking `min()` over all four edges means
corners need no special case and no seams can exist.

**Cursor tracking**: cosmic-comp implements `ext-image-copy-capture-v1`, whose
pointer cursor session reports cursor position independently of frame capture.
This needs no permissions, never grabs input, and captures no frames. Verify it
on any machine with `cargo run --example cursor_probe`.

## Key design decisions

- **No GNOME dependencies**: the GTK3/cairo/ksni stack is fully replaced
- **No /dev/input**: cursor position comes from the compositor, so no `input`
  group membership is needed and there is no drift from pointer acceleration
- **Proportional sizing**: glow depth is a fraction of the smaller screen
  dimension, so it looks identical at any resolution or scale
- **Physical pixels in uniforms**: `Program::draw` sees logical bounds, so the
  scale conversion happens in `Primitive::prepare`, the only place the viewport
  scale factor is available
- **`gnome-extension/` retained**: for reference; not used by the COSMIC build

## Status

- Compiles clean; unit tests cover the pure geometry, colour and config paths
- Runtime-verified on Pop!_OS 24.04 COSMIC (eDP-1, 3000x2000 @ 200%)
```

Also delete the stale "Verified libcosmic import paths" section's Canvas entry and replace it with the shader paths:

```rust
// Shader widget (requires libcosmic's "wgpu" feature)
use cosmic::iced_widget::shader::{self, Shader, Viewport};
use cosmic::iced_wgpu::wgpu;   // MUST come from here, not a direct wgpu dep
// impl shader::Program<Message> for MyProgram { type Primitive = ...; }
```

- [ ] **Step 5: Update README.md**

Update the requirements section: remove any mention of `input` group membership or the GNOME cursor extension being needed for cursor tracking, and note that cursor tracking requires a compositor implementing `ext-image-copy-capture-v1` (cosmic-comp does).

- [ ] **Step 6: Final end-to-end verification**

```bash
sudo cp target/release/ringlight /usr/local/bin/ && pkill -x cosmic-panel
```

Walk the full acceptance list:

1. Panel icon appears; the popup opens and closes.
2. Toggling **Enabled** shows and hides a soft glow on all four edges.
3. The glow reaches the true screen edge, including behind the panel.
4. No opaque bars, no hard inner line, no corner seams.
5. Clicks pass through the glow everywhere.
6. The cursor hole follows the cursor accurately at all four edges and corners.
7. Brightness, colour temperature, glow size and hole size all take effect.
8. Presets apply.
9. Settings survive `pkill -x cosmic-panel`.
10. Starting a video call with **Auto (camera)** on turns the glow on; ending it turns it off.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "chore: remove Quit, add cursor probe example, update docs

Quit left a dead panel icon; the Enabled toggle is the off switch. The
protocol probe is kept as an example so cursor support can be verified on
any machine."
```

---

## Self-Review

**Spec coverage:**

| Spec section | Task |
|---|---|
| Surface model (single, anchored, `exclusive_zone: -1`, empty input zone) | 1 |
| Shader renderer (`Program`/`Primitive`/`Pipeline`, premultiplied blend) | 2 |
| Spike before glow maths | 2 |
| Edge falloff, `MAX_ALPHA`, no summed passes | 4 |
| Proportional glow width and hole radius | 3 |
| Physical-pixel coordinate space | 2 (Step 3, `prepare`) |
| `cursor.rs` capture session, own connection, normalized output | 5 |
| `enter`/`leave` suppressing the hole | 5 (Step 6), 6 (Step 4) |
| Backoff on transient failure, permanent stop when unsupported | 5 |
| Rate-limited subscription | 6 |
| Cursor hole in shader | 6 |
| cosmic-config persistence, debounced writes | 7 |
| `Quit` removed | 8 |
| `mouse.rs` and `overlay.rs` deleted | 5, 1 |
| Probe kept as an example | 8 |
| Unit tests: falloff, widths, normalization, colour, serde | 3, 5, 7 |
| Manual verification checklist | 8 |
| Error handling: tiny-skia fallback detection | 2 (Step 6) |

Two spec items are deliberately handled differently, both simplifications rather than gaps:

- The spec proposed a standalone `falloff()` Rust function to unit-test. The falloff lives entirely in WGSL and has no Rust counterpart to test; testing a duplicated Rust copy would verify nothing about what actually renders. Covered by the Task 4 visual checks instead.
- The spec put proportional scaling in `settings.rs` returning pixels. It returns *fractions* instead, with the pixel conversion in `prepare`, because that is the only place the physical resolution is known. This removes the logical/physical confusion the spec flagged as its own risk rather than merely documenting it.

**Placeholder scan:** No TBDs, no "add error handling", no "similar to Task N". Every code step carries the literal code.

**Type consistency:** `CursorState { pos: [f32; 2], visible: bool }` is used identically in Tasks 5 and 6. `glow_fraction()`/`hole_fraction()` are defined in Task 3 and consumed in Tasks 4 and 6. `GlowProgram`'s five public fields are defined in Task 2 and populated in Tasks 2, 4 and 6 — Task 6 fills the two left as placeholders. `scale_to_min_dimension` is defined and tested in Task 3, and Task 3 Step 5 routes `prepare` through it so the tested function is the one that actually ships — Task 2 writes the multiplication inline only because the helper does not exist yet at that point.
