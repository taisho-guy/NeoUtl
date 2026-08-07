struct Uniforms
{
    float4x4 mvp;
    float opacity;
};

[[vk::binding(0, 0)]] ConstantBuffer<Uniforms> uniforms;
[[vk::binding(1, 0)]] Texture2D<float4> plane_y;
[[vk::binding(2, 0)]] Texture2D<float4> plane_uv;
[[vk::binding(3, 0)]] SamplerState media_sampler;

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
    float y = plane_y.Sample(media_sampler, in.uv).r;
    float2 uv = plane_uv.Sample(media_sampler, in.uv).rg;
    float y_full = (y - 16.0 / 255.0) * (255.0 / 219.0);
    float u = uv.x - 0.5;
    float v = uv.y - 0.5;
    float r = y_full + 1.5748 * v;
    float g = y_full - 0.1873 * u - 0.4681 * v;
    float b = y_full + 1.8556 * u;
    return float4(clamp(float3(r, g, b), float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), uniforms.opacity);
}
