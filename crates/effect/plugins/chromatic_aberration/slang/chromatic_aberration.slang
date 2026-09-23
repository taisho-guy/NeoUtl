struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float shift = u.p[0].x;
    float2 dims = tex_dims(input_tex);
    float2 dir = normalize(in.uv - float2(0.5, 0.5) + float2(1e-6, 0.0));
    float2 d = dir * shift / dims;
    float r = input_tex.Sample(input_sampler, in.uv + d).r;
    float g = input_tex.Sample(input_sampler, in.uv).g;
    float b = input_tex.Sample(input_sampler, in.uv - d).b;
    float a = input_tex.Sample(input_sampler, in.uv).a;
    return float4(r, g, b, a);
}
