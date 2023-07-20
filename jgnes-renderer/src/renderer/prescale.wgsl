struct PaddedRenderScale {
    value: u32,
    // Padding for WebGL
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
}

@group(0) @binding(0)
var texture_in: texture_2d<f32>;
@group(0) @binding(1)
var<uniform> render_scale: PaddedRenderScale;

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let top_left = vec2u(u32(round(position.x - 0.5)), u32(round(position.y - 0.5)));
    let input_position = vec2u(top_left.x / render_scale.value, top_left.y / render_scale.value);
    return textureLoad(texture_in, input_position, 0);
}