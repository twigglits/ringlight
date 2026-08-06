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
