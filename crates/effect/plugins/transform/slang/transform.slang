struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float scale = max(u.p[0].x, 0.0001);
    float rotation = radians(u.p[0].y);
    float2 centered = in.uv - float2(0.5, 0.5);
    float s = sin(-rotation);
    float c = cos(-rotation);
    float2 rotated = float2(centered.x * c - centered.y * s, centered.x * s + centered.y * c);
    float2 sampled_uv = rotated / scale + float2(0.5, 0.5);
    if (sampled_uv.x < 0.0 || sampled_uv.x > 1.0 || sampled_uv.y < 0.0 || sampled_uv.y > 1.0)
    {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return input_tex.Sample(input_sampler, sampled_uv);
}
