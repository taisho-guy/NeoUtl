struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let offset_x = u.p[0].x;
    let offset_y = u.p[0].y;
    let blur = u.p[0].z;
    let opacity = u.p[0].w;
    let dims = vec2<f32>(textureDimensions(input_tex));
    let d = vec2<f32>(offset_x, offset_y) / dims;
    let px = max(blur, 1.0) / dims;
    var shadow_a = 0.0;
    for (var i = -2; i <= 2; i++) {
        for (var j = -2; j <= 2; j++) {
            shadow_a += textureSample(input_tex, input_sampler, in.uv - d + vec2<f32>(f32(i), f32(j)) * px).a;
        }
    }
    shadow_a = (shadow_a / 25.0) * opacity;
    let c = textureSample(input_tex, input_sampler, in.uv);
    let shadow_color = vec3<f32>(0.0);
    let under = shadow_color * shadow_a * (1.0 - c.a);
    return vec4<f32>(c.rgb * c.a + under, max(c.a, shadow_a));
}
