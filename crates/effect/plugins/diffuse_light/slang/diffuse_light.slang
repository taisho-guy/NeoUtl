struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float intensity = u.p[0].x;
    float angle = radians(u.p[0].y);
    float2 n = float2(cos(angle), sin(angle));
    float2 centered = in.uv - float2(0.5, 0.5);
    float falloff = clamp(1.0 - abs(dot(centered, n)), 0.0, 1.0);
    float4 c = input_tex.Sample(input_sampler, in.uv);
    return float4(clamp(c.rgb + falloff * intensity, float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), c.a);
}
