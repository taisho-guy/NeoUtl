struct Uniforms
{
    float4x4 mvp;
    float opacity;
};

[[vk::binding(0, 0)]] ConstantBuffer<Uniforms> uniforms;
[[vk::binding(1, 0)]] Texture2D<float4> media_texture;
[[vk::binding(2, 0)]] SamplerState media_sampler;

struct VertexOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
VertexOutput vs_main(uint vertex_index : SV_VertexID)
{
    float2 positions[6] = {
        float2(-0.5, -0.5), float2(0.5, -0.5), float2(0.5, 0.5),
        float2(-0.5, -0.5), float2(0.5, 0.5), float2(-0.5, 0.5),
    };
    float2 uvs[6] = {
        float2(0.0, 1.0), float2(1.0, 1.0), float2(1.0, 0.0),
        float2(0.0, 1.0), float2(1.0, 0.0), float2(0.0, 0.0),
    };
    VertexOutput out;
    out.position = mul(uniforms.mvp, float4(positions[vertex_index], 0.0, 1.0));
    out.uv = uvs[vertex_index];
    return out;
}

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float4 sampled = media_texture.Sample(media_sampler, in.uv);
    float alpha = sampled.a * uniforms.opacity;
    if (alpha <= 0.0001)
    {
        discard;
    }
    return float4(sampled.rgb, alpha);
}
