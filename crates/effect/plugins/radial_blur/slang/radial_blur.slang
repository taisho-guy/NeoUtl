struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;
static const int TAPS = 9;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float2 center = float2(u.p[0].x, u.p[0].y);
    float strength = u.p[0].z / 1000.0;
    float2 dir = in.uv - center;
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    for (int i = 0; i < TAPS; i++)
    {
        float t = 1.0 - strength * (float(i) / float(TAPS - 1));
        acc += input_tex.Sample(input_sampler, center + dir * t);
    }
    return acc / float(TAPS);
}
