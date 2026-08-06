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
