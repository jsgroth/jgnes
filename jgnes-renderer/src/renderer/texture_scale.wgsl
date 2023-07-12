// Compute shaders

@group(0) @binding(0)
var texture_in: texture_2d<f32>;
@group(0) @binding(1)
var texture_out: texture_storage_2d<rgba8unorm, write>;

// This repetition is pathological (all of these shaders have the same body with different workgroup sizes), but I
// can't figure out a better way to do this

@compute @workgroup_size(2, 2, 1)
fn texture_scale_2x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(3, 3, 1)
fn texture_scale_3x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(4, 4, 1)
fn texture_scale_4x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(5, 5, 1)
fn texture_scale_5x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(6, 6, 1)
fn texture_scale_6x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(7, 7, 1)
fn texture_scale_7x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(8, 8, 1)
fn texture_scale_8x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(9, 9, 1)
fn texture_scale_9x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(10, 10, 1)
fn texture_scale_10x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(11, 11, 1)
fn texture_scale_11x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(12, 12, 1)
fn texture_scale_12x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(13, 13, 1)
fn texture_scale_13x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(14, 14, 1)
fn texture_scale_14x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(15, 15, 1)
fn texture_scale_15x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}

@compute @workgroup_size(16, 16, 1)
fn texture_scale_16x(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let pixel = textureLoad(texture_in, workgroup_id.xy, 0);
    textureStore(texture_out, global_invocation_id.xy, pixel);
}