struct PaddedRenderScale {
    value: u32,
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
}

@group(0) @binding(0)
var texture_in: texture_2d<f32>;
@group(0) @binding(1)
var<uniform> render_scale: PaddedRenderScale;

var<private> VERTICES: array<vec4f, 6> = array<vec4f, 6>(
    vec4f(-1.0, 1.0, 0.0, 1.0),
    vec4f(-1.0, -1.0, 0.0, 1.0),
    vec4f(1.0, -1.0, 0.0, 1.0),
    vec4f(1.0, -1.0, 0.0, 1.0),
    vec4f(1.0, 1.0, 0.0, 1.0),
    vec4f(-1.0, 1.0, 0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4f {
    return VERTICES[vertex_index];
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let top_left = vec2u(u32(round(position.x - 0.5)), u32(round(position.y - 0.5)));
    let input_position = vec2u(top_left.x / render_scale.value, top_left.y / render_scale.value);
    return textureLoad(texture_in, input_position, 0);
}