struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;
const TAPS: i32 = 9;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let angle = radians(u.p[0].x);
    let distance = u.p[0].y;
    let dims = vec2<f32>(textureDimensions(input_tex));
    let dir = vec2<f32>(cos(angle), sin(angle)) * distance / dims;
    var acc = vec4<f32>(0.0);
    for (var i = 0; i < TAPS; i++) {
        let t = (f32(i) / f32(TAPS - 1)) - 0.5;
        acc += textureSample(input_tex, input_sampler, in.uv + dir * t);
    }
    return acc / f32(TAPS);
}
