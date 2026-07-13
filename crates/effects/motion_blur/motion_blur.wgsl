struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;
const TAPS: i32 = 9;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let shutter = clamp(u.p[0].x, 0.0, 360.0) / 360.0;
    let dims = vec2<f32>(textureDimensions(input_tex));
    let extent = shutter * 40.0 / dims.x;
    var acc = vec4<f32>(0.0);
    for (var i = 0; i < TAPS; i++) {
        let t = (f32(i) / f32(TAPS - 1)) - 0.5;
        acc += textureSample(input_tex, input_sampler, in.uv + vec2<f32>(extent * t, 0.0));
    }
    return acc / f32(TAPS);
}
