return {
  id = "lua_glow",
  name = "Glow (Lua)",
  category = "Light",
  params = {
    { key = "intensity", label = "強度", kind = "float", min = 0.0, max = 5.0, default = 1.0 },
    { key = "threshold", label = "しきい値", kind = "float", min = 0.0, max = 1.0, default = 0.7 },
  },
  wgsl = [[
struct Uniforms { p: array<vec4<f32>, 1>, };
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, 1.0), vec2<f32>(3.0, 1.0), vec2<f32>(-1.0, -3.0),
    );
    var out: VertexOut;
    out.position = vec4<f32>(positions[index], 0.0, 1.0);
    out.uv = vec2<f32>(positions[index].x * 0.5 + 0.5, 0.5 - positions[index].y * 0.5);
    return out;
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;
@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let intensity = u.p[0].x;
    let threshold = u.p[0].y;
    let c = textureSample(src_tex, src_sampler, uv);
    let luma = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let boost = max(luma - threshold, 0.0) * intensity;
    return vec4<f32>(c.rgb + vec3<f32>(boost), c.a);
}
]],
}
