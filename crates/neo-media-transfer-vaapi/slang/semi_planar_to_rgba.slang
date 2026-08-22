[[vk::binding(0, 0)]] Texture2D<float> plane_y;
[[vk::binding(1, 0)]] Texture2D<float2> plane_uv;
[[vk::binding(2, 0)]] SamplerState chroma_sampler;
[[vk::binding(3, 0)]] [format("rgba16f")] RWTexture2D<float4> output;

struct ColorTags
{
    uint matrix_coefficients;
    uint transfer_characteristics;
    uint color_primaries;
    uint full_range;
};
[[vk::push_constant]] ColorTags tags;

static const uint MATRIX_BT709 = 0;
static const uint MATRIX_BT2020_NCL = 1;
static const uint MATRIX_SMPTE170M = 2;

static const uint TRANSFER_BT709 = 0;
static const uint TRANSFER_SMPTE2084 = 1;
static const uint TRANSFER_ARIB_STD_B67 = 2;

static const uint PRIMARIES_BT2020 = 1;

static const float HLG_SYSTEM_GAMMA = 1.2;
static const float HDR_REFERENCE_WHITE_NITS = 100.0;
static const float PQ_MAX_NITS = 10000.0;
static const float PQ_M1 = 0.1593017578125;
static const float PQ_M2 = 78.84375;
static const float PQ_C1 = 0.8359375;
static const float PQ_C2 = 18.8515625;
static const float PQ_C3 = 18.6875;

float2 kr_kb_of(uint matrix_coefficients)
{
    if (matrix_coefficients == MATRIX_BT2020_NCL) { return float2(0.2627, 0.0593); }
    if (matrix_coefficients == MATRIX_SMPTE170M) { return float2(0.299, 0.114); }
    return float2(0.2126, 0.0722);
}

float3 yuv_to_rgb(float y, float2 uv, uint matrix_coefficients, bool full_range)
{
    float2 kr_kb = kr_kb_of(matrix_coefficients);
    float kr = kr_kb.x;
    float kb = kr_kb.y;
    float kg = 1.0 - kr - kb;

    float y_full = full_range ? y : (y - 16.0 / 255.0) * (255.0 / 219.0);
    float2 c = full_range
        ? (uv - float2(0.5, 0.5))
        : (uv - float2(128.0 / 255.0, 128.0 / 255.0)) * (255.0 / 224.0);
    float cb = c.x;
    float cr = c.y;

    float r = y_full + 2.0 * (1.0 - kr) * cr;
    float b = y_full + 2.0 * (1.0 - kb) * cb;
    float g = y_full - (kb / kg) * 2.0 * (1.0 - kb) * cb - (kr / kg) * 2.0 * (1.0 - kr) * cr;
    return float3(r, g, b);
}

float3 eotf_to_linear(float3 v, uint transfer_characteristics)
{
    float3 c = max(v, float3(0.0, 0.0, 0.0));
    if (transfer_characteristics == TRANSFER_SMPTE2084)
    {
        float3 cm1 = pow(c, float3(1.0 / PQ_M2, 1.0 / PQ_M2, 1.0 / PQ_M2));
        float3 num = max(cm1 - PQ_C1, float3(0.0, 0.0, 0.0));
        float3 den = PQ_C2 - PQ_C3 * cm1;
        float3 nits = pow(num / den, float3(1.0 / PQ_M1, 1.0 / PQ_M1, 1.0 / PQ_M1)) * PQ_MAX_NITS;
        return nits / HDR_REFERENCE_WHITE_NITS;
    }
    if (transfer_characteristics == TRANSFER_ARIB_STD_B67)
    {
        float3 scene = c <= float3(0.5, 0.5, 0.5)
            ? c * c / 3.0
            : (exp((c - 0.55991073) / 0.17883277) + 0.02372241) / 12.0;
        float peak_ratio = 1000.0 / HDR_REFERENCE_WHITE_NITS;
        return scene * pow(max(scene.x + scene.y + scene.z, 1e-6) / 3.0, HLG_SYSTEM_GAMMA - 1.0) * peak_ratio;
    }
    return pow(c, float3(2.4, 2.4, 2.4));
}

float3 linear_to_oetf_bt709(float3 v)
{
    float3 c = clamp(v, float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0));
    return pow(c, float3(1.0 / 2.4, 1.0 / 2.4, 1.0 / 2.4));
}

float3 bt2020_to_bt709_linear(float3 v)
{
    float3x3 m = float3x3(
        1.6605, -0.5876, -0.0728,
        -0.1246, 1.1329, -0.0083,
        -0.0182, -0.1006, 1.1187);
    return mul(m, v);
}

float3 tonemap_reinhard(float3 v)
{
    return v / (float3(1.0, 1.0, 1.0) + v);
}

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    uint out_width;
    uint out_height;
    output.GetDimensions(out_width, out_height);
    if (id.x >= out_width || id.y >= out_height)
    {
        return;
    }

    float2 uv_norm = (float2(id.xy) + float2(0.5, 0.5)) / float2(out_width, out_height);
    float y_sample = plane_y.Load(int3(id.xy, 0)).r;
    float2 uv_sample = plane_uv.SampleLevel(chroma_sampler, uv_norm, 0.0).rg;
    bool full_range = tags.full_range != 0;

    float3 encoded = yuv_to_rgb(y_sample, uv_sample, tags.matrix_coefficients, full_range);
    float3 linear = eotf_to_linear(encoded, tags.transfer_characteristics);

    if (tags.color_primaries == PRIMARIES_BT2020)
    {
        linear = bt2020_to_bt709_linear(linear);
    }
    if (tags.transfer_characteristics == TRANSFER_SMPTE2084 || tags.transfer_characteristics == TRANSFER_ARIB_STD_B67)
    {
        linear = tonemap_reinhard(max(linear, float3(0.0, 0.0, 0.0)));
    }

    float3 display = linear_to_oetf_bt709(linear);
    output[id.xy] = float4(display, 1.0);
}
