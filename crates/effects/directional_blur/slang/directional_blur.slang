struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;
static const int TAPS = 9;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float angle = radians(u.p[0].x);
    float distance_ = u.p[0].y;
    float2 dims = tex_dims(input_tex);
    float2 dir = float2(cos(angle), sin(angle)) * distance_ / dims;
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    for (int i = 0; i < TAPS; i++)
    {
        float t = (float(i) / float(TAPS - 1)) - 0.5;
        acc += input_tex.Sample(input_sampler, in.uv + dir * t);
    }
    return acc / float(TAPS);
}
