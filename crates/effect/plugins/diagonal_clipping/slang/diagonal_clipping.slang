struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float angle = radians(u.p[0].x);
    float offset = u.p[0].y;
    float2 n = float2(cos(angle), sin(angle));
    float2 centered = in.uv - float2(0.5, 0.5);
    float d = dot(centered, n) - offset * 0.5;
    float4 c = input_tex.Sample(input_sampler, in.uv);
    return d <= 0.0 ? c : float4(0.0, 0.0, 0.0, 0.0);
}
