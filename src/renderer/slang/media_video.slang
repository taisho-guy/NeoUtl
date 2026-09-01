struct Uniforms
{
    float4x4 mvp;
    float opacity;
    uint color_matrix; // 0=BT601, 1=BT709, 2=BT2020
    uint color_range;  // 0=Limited, 1=Full
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

// ITU-R BT.601/BT.709/BT.2020 YCbCr->RGB係数（非定数行列版）
float3 yuv_to_rgb(float y, float u, float v, uint color_matrix)
{
    float kr, kb;
    if (color_matrix == 0)
    {
        kr = 0.299; kb = 0.114;
    }
    else if (color_matrix == 2)
    {
        kr = 0.2627; kb = 0.0593;
    }
    else
    {
        kr = 0.2126; kb = 0.0722;
    }
    float kg = 1.0 - kr - kb;
    float r = y + 2.0 * (1.0 - kr) * v;
    float b = y + 2.0 * (1.0 - kb) * u;
    float g = (y - kr * r - kb * b) / kg;
    return float3(r, g, b);
}

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float y_raw = plane_y.Sample(media_sampler, in.uv).r;
    float2 uv_raw = plane_uv.Sample(media_sampler, in.uv).rg;

    float y;
    float u;
    float v;
    if (uniforms.color_range == 1)
    {
        // フルレンジ: 0-255がそのまま0.0-1.0
        y = y_raw;
        u = uv_raw.x - 0.5;
        v = uv_raw.y - 0.5;
    }
    else
    {
        // リミテッドレンジ: Y=16-235, Cb/Cr=16-240
        y = (y_raw - 16.0 / 255.0) * (255.0 / 219.0);
        u = (uv_raw.x - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
        v = (uv_raw.y - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
    }

    float3 rgb = yuv_to_rgb(y, u, v, uniforms.color_matrix);
    return float4(clamp(rgb, float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), uniforms.opacity);
}
