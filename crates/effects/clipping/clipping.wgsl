struct Uniforms { p: array<vec4<f32>, 2> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let left = u.p[0].x;
    let top = u.p[0].y;
    let right = u.p[0].z;
    let bottom = u.p[0].w;
    let invert = u.p[1].x > 0.5;
    let inside = in.uv.x >= left && in.uv.x <= right && in.uv.y >= top && in.uv.y <= bottom;
    let visible = select(inside, !inside, invert);
    let c = textureSample(input_tex, input_sampler, in.uv);
    return select(vec4<f32>(0.0), c, visible);
}
