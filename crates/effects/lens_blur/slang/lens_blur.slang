struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;
static const int RINGS = 8;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float radius = u.p[0].x;
    float2 dims = tex_dims(input_tex);
    float2 px = float2(radius, radius) / dims;
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    float wsum = 0.0;
    for (int i = 0; i < RINGS; i++)
    {
        float a = (float(i) / float(RINGS)) * 2.0 * 3.14159265;
        float2 o = float2(cos(a), sin(a)) * px;
        acc += input_tex.Sample(input_sampler, in.uv + o);
        wsum += 1.0;
    }
    acc += input_tex.Sample(input_sampler, in.uv);
    wsum += 1.0;
    return acc / wsum;
}
