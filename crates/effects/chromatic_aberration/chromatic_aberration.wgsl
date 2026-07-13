struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let shift = u.p[0].x;
    let dims = vec2<f32>(textureDimensions(input_tex));
    let dir = normalize(in.uv - vec2<f32>(0.5, 0.5) + vec2<f32>(1e-6, 0.0));
    let d = dir * shift / dims;
    let r = textureSample(input_tex, input_sampler, in.uv + d).r;
    let g = textureSample(input_tex, input_sampler, in.uv).g;
    let b = textureSample(input_tex, input_sampler, in.uv - d).b;
    let a = textureSample(input_tex, input_sampler, in.uv).a;
    return vec4<f32>(r, g, b, a);
}
