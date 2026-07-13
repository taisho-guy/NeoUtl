struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;
const TAPS: i32 = 9;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let center = vec2<f32>(u.p[0].x, u.p[0].y);
    let strength = u.p[0].z / 1000.0;
    let dir = in.uv - center;
    var acc = vec4<f32>(0.0);
    for (var i = 0; i < TAPS; i++) {
        let t = 1.0 - strength * (f32(i) / f32(TAPS - 1));
        acc += textureSample(input_tex, input_sampler, center + dir * t);
    }
    return acc / f32(TAPS);
}
