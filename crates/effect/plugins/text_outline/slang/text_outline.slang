struct Uniforms { float4 p[5]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float width = u.p[0].x;
    float4 outline_color = float4(u.p[1].x, u.p[2].x, u.p[3].x, u.p[4].x);
    float4 c = input_tex.Sample(input_sampler, in.uv);
    if (c.a > 0.5)
    {
        return c;
    }
    float2 dims = tex_dims(input_tex);
    float2 px = float2(max(width, 1.0), max(width, 1.0)) / dims;
    float found = 0.0;
    for (int i = 0; i < 8; i++)
    {
        float angle = (float(i) / 8.0) * 6.28318530718;
        float2 offset = float2(cos(angle), sin(angle)) * px;
        found = max(found, input_tex.Sample(input_sampler, in.uv + offset).a);
    }
    if (found > 0.5)
    {
        return float4(outline_color.rgb * outline_color.a, outline_color.a);
    }
    return c;
}
