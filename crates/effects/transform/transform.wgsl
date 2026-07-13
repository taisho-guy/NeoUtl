struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let scale = max(u.p[0].x, 0.0001);
    let rotation = radians(u.p[0].y);
    let centered = in.uv - vec2<f32>(0.5, 0.5);
    let s = sin(-rotation);
    let c = cos(-rotation);
    let rotated = vec2<f32>(centered.x * c - centered.y * s, centered.x * s + centered.y * c);
    let sampled_uv = rotated / scale + vec2<f32>(0.5, 0.5);
    if (sampled_uv.x < 0.0 || sampled_uv.x > 1.0 || sampled_uv.y < 0.0 || sampled_uv.y > 1.0) {
        return vec4<f32>(0.0);
    }
    return textureSample(input_tex, input_sampler, sampled_uv);
}
