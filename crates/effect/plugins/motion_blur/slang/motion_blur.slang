struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;
static const int TAPS = 9;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float shutter = clamp(u.p[0].x, 0.0, 360.0) / 360.0;
    float2 dims = tex_dims(input_tex);
    float extent = shutter * 40.0 / dims.x;
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    for (int i = 0; i < TAPS; i++)
    {
        float t = (float(i) / float(TAPS - 1)) - 0.5;
        acc += input_tex.Sample(input_sampler, in.uv + float2(extent * t, 0.0));
    }
    return acc / float(TAPS);
}
