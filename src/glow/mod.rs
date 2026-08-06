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
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
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

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
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
