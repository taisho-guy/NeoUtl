struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let angle = radians(u.p[0].x);
    let offset = u.p[0].y;
    let n = vec2<f32>(cos(angle), sin(angle));
    let centered = in.uv - vec2<f32>(0.5, 0.5);
    let d = dot(centered, n) - offset * 0.5;
    let c = textureSample(input_tex, input_sampler, in.uv);
    return select(vec4<f32>(0.0), c, d <= 0.0);
}
