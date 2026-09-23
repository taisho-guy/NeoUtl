struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float offset_x = u.p[0].x;
    float offset_y = u.p[0].y;
    float blur = u.p[0].z;
    float opacity = u.p[0].w;
    float2 dims = tex_dims(input_tex);
    float2 d = float2(offset_x, offset_y) / dims;
    float2 px = float2(max(blur, 1.0), max(blur, 1.0)) / dims;
    float shadow_a = 0.0;
    for (int i = -2; i <= 2; i++)
    {
        for (int j = -2; j <= 2; j++)
        {
            shadow_a += input_tex.Sample(input_sampler, in.uv - d + float2(float(i), float(j)) * px).a;
        }
    }
    shadow_a = (shadow_a / 25.0) * opacity;
    float4 c = input_tex.Sample(input_sampler, in.uv);
    float3 shadow_color = float3(0.0, 0.0, 0.0);
    float3 under = shadow_color * shadow_a * (1.0 - c.a);
    return float4(c.rgb * c.a + under, max(c.a, shadow_a));
}
