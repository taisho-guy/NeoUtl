// 2plane系(NV12/P010LE/P012LE/P016LE、Y:R8|R16Uint、UV:Rg8|Rg16Uint)→RGBA8合成共通シェーダー。
// NV12(8bit)はストレージ幅8bit・有効bit幅8bitで上位詰めなし。
// P010/P012/P016(10/12/16bit)はストレージ幅16bit・有効bit幅bit_depthで上位詰め、下位0埋め。
// R16Unorm/Rg16Unormはwgpu Features::TEXTURE_FORMAT_16BIT_NORM必須のため、
// 同フィーチャ有効化有無を問わず動作するUint系(コア仕様、フィーチャ不要)を採用する。
// Uintテクスチャはtextureフィルタリング(Sampler)非対応のため、
// Y平面は等倍のためLoadのみ、UV平面(半解像度)は手動バイリニア補間を行う。
// storage_bits(格納幅)とbit_depth(有効値幅)をuniformで受け取り正規化式を単一シェーダーで共有する。
struct ColorUniforms
{
    uint color_matrix; // 0=BT601, 1=BT709, 2=BT2020
    uint color_range;  // 0=Limited, 1=Full
    uint bit_depth;    // 8, 10, 12, 16のいずれか
    uint storage_bits; // 8または16（テクスチャ格納幅）
};

[[vk::binding(0, 0)]] ConstantBuffer<ColorUniforms> uniforms;
[[vk::binding(1, 0)]] Texture2D<uint> plane_y;
[[vk::binding(2, 0)]] Texture2D<uint2> plane_uv;

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

// 格納値はstorage_bits中上位bit_depth bitに有効値、下位(storage_bits-bit_depth)bitは0埋め。
// >>(storage_bits-bit_depth)でbit_depth bit値へ正規化し、(2^bit_depth - 1)で除算する。
// 8bit(NV12)はstorage_bits==bit_depthのためshift=0（上位詰めなし、素通し）。
uint normalize_shift()
{
    return uniforms.storage_bits - uniforms.bit_depth;
}

float normalize_max()
{
    return float((1u << uniforms.bit_depth) - 1u);
}

float load_y_normalized(int2 coord)
{
    uint raw = plane_y.Load(int3(coord, 0));
    return float(raw >> normalize_shift()) / normalize_max();
}

float2 load_uv_normalized(int2 coord)
{
    uint2 raw = plane_uv.Load(int3(coord, 0));
    return float2(raw >> normalize_shift()) / normalize_max();
}

float2 sample_uv_bilinear(float2 uv_pos, uint uv_width, uint uv_height)
{
    float2 dims_f = float2(uv_width, uv_height);
    float2 pixel_pos = uv_pos * dims_f - float2(0.5, 0.5);
    float2 base = floor(pixel_pos);
    float2 frac_part = pixel_pos - base;
    int2 max_coord = int2(uv_width, uv_height) - int2(1, 1);

    int2 c00 = clamp(int2(base), int2(0, 0), max_coord);
    int2 c10 = clamp(int2(base) + int2(1, 0), int2(0, 0), max_coord);
    int2 c01 = clamp(int2(base) + int2(0, 1), int2(0, 0), max_coord);
    int2 c11 = clamp(int2(base) + int2(1, 1), int2(0, 0), max_coord);

    float2 v00 = load_uv_normalized(c00);
    float2 v10 = load_uv_normalized(c10);
    float2 v01 = load_uv_normalized(c01);
    float2 v11 = load_uv_normalized(c11);

    float2 top = lerp(v00, v10, frac_part.x);
    float2 bottom = lerp(v01, v11, frac_part.x);
    return lerp(top, bottom, frac_part.y);
}

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    uint y_width, y_height;
    plane_y.GetDimensions(y_width, y_height);
    uint uv_width, uv_height;
    plane_uv.GetDimensions(uv_width, uv_height);

    int2 y_max_coord = int2(y_width, y_height) - int2(1, 1);
    int2 y_coord = clamp(int2(in.uv * float2(y_width, y_height)), int2(0, 0), y_max_coord);

    float y_raw = load_y_normalized(y_coord);
    float2 uv_raw = sample_uv_bilinear(in.uv, uv_width, uv_height);

    float y;
    float u;
    float v;
    if (uniforms.color_range == 1)
    {
        y = y_raw;
        u = uv_raw.x - 0.5;
        v = uv_raw.y - 0.5;
    }
    else
    {
        y = (y_raw - 16.0 / 255.0) * (255.0 / 219.0);
        u = (uv_raw.x - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
        v = (uv_raw.y - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
    }

    float3 rgb = yuv_to_rgb(y, u, v, uniforms.color_matrix);
    return float4(clamp(rgb, float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), 1.0);
}
