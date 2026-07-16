struct Uniforms {
    mvp: mat4x4<f32>,
    opacity: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var plane_y: texture_2d<f32>;
@group(0) @binding(2) var plane_uv: texture_2d<f32>;
@group(0) @binding(3) var media_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, 0.5),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 0.0),
    );
    var out: VertexOutput;
    out.position = uniforms.mvp * vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

// NV12(BT.709, limited-range) -> RGB変換。
// plane_y: R8 (輝度), plane_uv: RG8 (色差、1/2解像度)。
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(plane_y, media_sampler, in.uv).r;
    let uv = textureSample(plane_uv, media_sampler, in.uv).rg;
    let y_full = (y - 16.0 / 255.0) * (255.0 / 219.0);
    let u = uv.x - 0.5;
    let v = uv.y - 0.5;
    let r = y_full + 1.5748 * v;
    let g = y_full - 0.1873 * u - 0.4681 * v;
    let b = y_full + 1.8556 * u;
    return vec4<f32>(clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0)), uniforms.opacity);
}

