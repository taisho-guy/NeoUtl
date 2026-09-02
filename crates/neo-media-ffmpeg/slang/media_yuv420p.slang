// YUV420P(3plane)→RGBA8合成専用シェーダー。
// wgpuにネイティブ3plane multi-planarテクスチャ型が存在しないため、
// neo-media-ffmpegクレート内部でGpuFrame昇格時に単発合成し、
// 合成後のRGBA8テクスチャのみをVideoSourceトレイト境界へ返す。
struct ColorUniforms
{
    uint color_matrix; // 0=BT601, 1=BT709, 2=BT2020
    uint color_range;  // 0=Limited, 1=Full
};

[[vk::binding(0, 0)]] ConstantBuffer<ColorUniforms> uniforms;
[[vk::binding(1, 0)]] Texture2D<float4> plane_y;
[[vk::binding(2, 0)]] Texture2D<float4> plane_u;
[[vk::binding(3, 0)]] Texture2D<float4> plane_v;
[[vk::binding(4, 0)]] SamplerState media_sampler;

struct VertexOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
VertexOutput vs_main(uint vertex_index : SV_VertexID)
{
    float2 positions[3] = {
        float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0),
    };
    float2 uvs[3] = {
        float2(0.0, 1.0), float2(2.0, 1.0), float2(0.0, -1.0),
    };
    VertexOutput out;
    out.position = float4(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

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
    float u_raw = plane_u.Sample(media_sampler, in.uv).r;
    float v_raw = plane_v.Sample(media_sampler, in.uv).r;

    float y;
    float u;
    float v;
    if (uniforms.color_range == 1)
    {
        y = y_raw;
        u = u_raw - 0.5;
        v = v_raw - 0.5;
    }
    else
    {
        y = (y_raw - 16.0 / 255.0) * (255.0 / 219.0);
        u = (u_raw - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
        v = (v_raw - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
    }

    float3 rgb = yuv_to_rgb(y, u, v, uniforms.color_matrix);
    return float4(clamp(rgb, float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), 1.0);
}
