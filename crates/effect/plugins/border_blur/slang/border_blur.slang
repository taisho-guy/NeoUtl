struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float radius = u.p[0].x;
    float border_width = u.p[0].y;
    float2 dims = tex_dims(input_tex);
    float d = min(min(in.uv.x, 1.0 - in.uv.x), min(in.uv.y, 1.0 - in.uv.y)) * min(dims.x, dims.y);
    float t = clamp(1.0 - d / max(border_width, 1.0), 0.0, 1.0);
    float px = radius * t / max(dims.x, dims.y);
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    float wsum = 0.0;
    for (int i = -2; i <= 2; i++)
    {
        for (int j = -2; j <= 2; j++)
        {
            float w = 1.0;
            acc += input_tex.Sample(input_sampler, in.uv + float2(float(i), float(j)) * px) * w;
            wsum += w;
        }
    }
    return acc / wsum;
}
