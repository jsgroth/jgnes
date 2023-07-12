// Vertex shaders

struct VertexInput {
    @location(0) position: vec2f,
    @location(1) texture_coords: vec2f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texture_coords: vec2f,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.position = vec4f(input.position, 0.0, 1.0);
    out.texture_coords = input.texture_coords;

    return out;
}

// Fragment shaders

struct FragmentGlobals {
    viewport_x: u32,
    viewport_y: u32,
    viewport_width: u32,
    viewport_height: u32,
    nes_visible_height: u32,
    // Padding required for WebGL, which requires structs to be aligned to 16-byte boundaries
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;
@group(0) @binding(2)
var<uniform> fs_globals: FragmentGlobals;

@fragment
fn basic_fs(input: VertexOutput) -> @location(0) vec4f {
    return textureSample(t_diffuse, s_diffuse, input.texture_coords);
}

const BLACK = vec4f(0.0, 0.0, 0.0, 1.0);

@fragment
fn scanlines_fs(input: VertexOutput) -> @location(0) vec4f {
    let vp_line = u32(round(input.position.y - 0.5)) - fs_globals.viewport_y;
    let crt_line = 2u * fs_globals.nes_visible_height * vp_line / fs_globals.viewport_height;

    let is_odd_line = crt_line % 2u == 0u;

    let color = textureSample(t_diffuse, s_diffuse, input.texture_coords);
    let filtered_color = select(BLACK, color, is_odd_line);

    // Ignore result of scanline filtering if viewport height is less than 2x native frame height
    let vp_too_small = fs_globals.viewport_height < 2u * fs_globals.nes_visible_height;
    return select(filtered_color, color, vp_too_small);
}