struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = u.p[0].x;
    let border_width = u.p[0].y;
    let dims = vec2<f32>(textureDimensions(input_tex));
    let d = min(min(in.uv.x, 1.0 - in.uv.x), min(in.uv.y, 1.0 - in.uv.y)) * min(dims.x, dims.y);
    let t = clamp(1.0 - d / max(border_width, 1.0), 0.0, 1.0);
    let px = radius * t / max(dims.x, dims.y);
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = -2; i <= 2; i++) {
        for (var j = -2; j <= 2; j++) {
            let w = 1.0;
            acc += textureSample(input_tex, input_sampler, in.uv + vec2<f32>(f32(i), f32(j)) * px) * w;
            wsum += w;
        }
    }
    return acc / wsum;
}
