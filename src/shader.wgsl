struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

struct Uniforms {
    angle: f32,
    padding: vec3<f32>,
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    
    var positions = array<vec3<f32>, 4>(
        vec3<f32>( 0.0,  0.5,  0.0),
        vec3<f32>(-0.4, -0.3,  0.3),
        vec3<f32>( 0.4, -0.3,  0.3),
        vec3<f32>( 0.0, -0.3, -0.5)
    );

    var indices = array<u32, 12>(
        0u, 1u, 2u,
        0u, 2u, 3u,
        0u, 3u, 1u,
        1u, 3u, 2u
    );

    let pos = positions[indices[in_vertex_index]];

    let s = sin(uniforms.angle);
    let c = cos(uniforms.angle);
    let rotated_x = pos.x * c - pos.z * s;
    let rotated_z = pos.x * s + pos.z * c;

    out.position = vec4<f32>(rotated_x, pos.y, rotated_z, 1.0);
    
    var colors = array<vec4<f32>, 4>(
        vec4<f32>(1.0, 0.0, 0.0, 1.0),
        vec4<f32>(0.0, 1.0, 0.0, 1.0),
        vec4<f32>(0.0, 0.0, 1.0, 1.0),
        vec4<f32>(1.0, 1.0, 0.0, 1.0)
    );
    out.color = colors[indices[in_vertex_index]];

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}