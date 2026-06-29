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
    
    var p = array<vec3<f32>, 8>(
        vec3<f32>(-0.3, -0.3,  0.3), vec3<f32>( 0.3, -0.3,  0.3),
        vec3<f32>( 0.3,  0.3,  0.3), vec3<f32>(-0.3,  0.3,  0.3),
        vec3<f32>(-0.3, -0.3, -0.3), vec3<f32>( 0.3, -0.3, -0.3),
        vec3<f32>( 0.3,  0.3, -0.3), vec3<f32>(-0.3,  0.3, -0.3)
    );

    var indices = array<u32, 36>(
        0u, 1u, 2u, 0u, 2u, 3u,
        1u, 5u, 6u, 1u, 6u, 2u,
        5u, 4u, 7u, 5u, 7u, 6u,
        4u, 0u, 3u, 4u, 3u, 7u,
        3u, 2u, 6u, 3u, 6u, 7u,
        4u, 5u, 1u, 4u, 1u, 0u
    );

    let pos = p[indices[in_vertex_index]];
    
    let s = sin(uniforms.angle);
    let c = cos(uniforms.angle);
    let ry_x = pos.x * c - pos.z * s;
    let ry_z = pos.x * s + pos.z * c;
    let rx_y = pos.y * c - ry_z * s;
    let rx_z = pos.y * s + ry_z * c;

    out.position = vec4<f32>(ry_x, rx_y, rx_z, 1.0);
    out.color = vec4<f32>(pos.x + 0.5, pos.y + 0.5, pos.z + 0.5, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
