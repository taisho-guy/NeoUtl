struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float threshold = u.p[0].x;
    float angle = radians(u.p[0].y);
    float2 dims = tex_dims(input_tex);
    float2 dir = float2(cos(angle), sin(angle));
    float4 c = input_tex.Sample(input_sampler, in.uv);
    float luma = dot(c.rgb, float3(0.299, 0.587, 0.114));
    bool over = luma > threshold;
    float shift = (over ? 12.0 : 0.0) / dims.x;
    float4 sorted = input_tex.Sample(input_sampler, in.uv + dir * shift);
    return over ? sorted : c;
}
