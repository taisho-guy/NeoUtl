struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;
const RINGS: i32 = 8;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = u.p[0].x;
    let dims = vec2<f32>(textureDimensions(input_tex));
    let px = radius / dims;
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = 0; i < RINGS; i++) {
        let a = (f32(i) / f32(RINGS)) * 2.0 * 3.14159265;
        let o = vec2<f32>(cos(a), sin(a)) * px;
        acc += textureSample(input_tex, input_sampler, in.uv + o);
        wsum += 1.0;
    }
    acc += textureSample(input_tex, input_sampler, in.uv);
    wsum += 1.0;
    return acc / wsum;
}
