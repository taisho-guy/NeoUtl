struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let cell = max(u.p[0].x, 1.0);
    let dims = vec2<f32>(textureDimensions(input_tex));
    let cell_uv = cell / dims;
    let snapped = (floor(in.uv / cell_uv) + vec2<f32>(0.5, 0.5)) * cell_uv;
    return textureSample(input_tex, input_sampler, snapped);
}
