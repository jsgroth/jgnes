// Vertex shaders

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) texture_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texture_coords: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = vec4<f32>(input.position, 0.0, 1.0);
    out.texture_coords = input.texture_coords;

    return out;
}

// Fragment shaders

struct FragmentGlobals {
    viewport_x: vec2<u32>,
    viewport_y: vec2<u32>,
    nes_visible_height: u32,
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;
@group(0) @binding(2)
var<uniform> fs_globals: FragmentGlobals;

@fragment
fn basic_fs(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, input.texture_coords);
}

const BLACK = vec4<f32>(0.0, 0.0, 0.0, 1.0);

@fragment
fn crt_scanlines_fs(input: VertexOutput) -> @location(0) vec4<f32> {
    let vp_y = u32(round(input.clip_position.y - 0.5)) - fs_globals.viewport_y[0];
    let crt_line = 2u * fs_globals.nes_visible_height * vp_y / (fs_globals.viewport_y[1] - fs_globals.viewport_y[0]);

    let is_even_line = crt_line % 2u == 1u;

    let color = textureSample(t_diffuse, s_diffuse, input.texture_coords);

    return select(color, BLACK, is_even_line);
}