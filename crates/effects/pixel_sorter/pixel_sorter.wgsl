struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let threshold = u.p[0].x;
    let angle = radians(u.p[0].y);
    let dims = vec2<f32>(textureDimensions(input_tex));
    let dir = vec2<f32>(cos(angle), sin(angle));
    let c = textureSample(input_tex, input_sampler, in.uv);
    let luma = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let shift = select(0.0, 12.0, luma > threshold) / dims.x;
    let sorted = textureSample(input_tex, input_sampler, in.uv + dir * shift);
    return select(c, sorted, luma > threshold);
}
