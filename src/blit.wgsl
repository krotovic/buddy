// Copies Vello's output to the window surface.
//
// Vello writes straight (non-premultiplied) alpha. Transparent windows on
// both Windows (DirectComposition) and macOS (Core Animation) composite
// premultiplied alpha, so the colour is multiplied by alpha here.

@group(0) @binding(0) var source: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(source, vec2<i32>(pos.xy), 0);
    return vec4<f32>(c.rgb * c.a, c.a);
}
