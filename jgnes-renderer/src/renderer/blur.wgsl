struct BlurGlobals {
    texture_width: u32,
    texture_height: u32,
    // 0 = horizontal, 1 = vertical
    blur_direction: u32,
}

@group(0) @binding(0)
var texture_in: texture_2d<f32>;
@group(0) @binding(1)
var<uniform> globals: BlurGlobals;
@group(0) @binding(2)
var<storage, read> weights: array<f32>;

fn blur_tap(position: vec2u, shift: i32) -> vec3f {
    let horizontal = globals.blur_direction == 0u;
    let shift_vec = select(vec2i(0, shift), vec2i(shift, 0), horizontal);

    let shifted = vec2i(position) + shift_vec;
    let clamped = clamp(shifted, vec2i(0, 0), vec2i(i32(globals.texture_width) - 1, i32(globals.texture_height) - 1));
    return textureLoad(texture_in, vec2u(clamped), 0).rgb;
}

@fragment
fn fs_main(@builtin(position) center_position: vec4f) -> @location(0) vec4f {
    let tl_position = vec2u(u32(round(center_position.x - 0.5)), u32(round(center_position.y - 0.5)));
    let center = i32(arrayLength(&weights)) / 2;

    var color = weights[center] * textureLoad(texture_in, tl_position, 0).rgb;

    for (var i = 1; i <= center; i += 1) {
        color += weights[center - i] * blur_tap(tl_position, -i);
        color += weights[center + i] * blur_tap(tl_position, i);
    }

    return vec4f(color, 1.0);
}
