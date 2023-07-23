// A vertex shader that simply draws a quad covering the entire output area

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