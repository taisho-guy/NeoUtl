
[[vk::binding(0, 0)]] Texture2D<float> plane_y;
[[vk::binding(1, 0)]] Texture2D<float2> plane_uv;
[[vk::binding(2, 0)]] SamplerState chroma_sampler;
[[vk::binding(3, 0)]] RWTexture2D<float4> output;

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

    float y_full = (y_sample - 16.0 / 255.0) * (255.0 / 219.0);
    float u = uv_sample.x - 0.5;
    float v = uv_sample.y - 0.5;
    float r = y_full + 1.5748 * v;
    float g = y_full - 0.1873 * u - 0.4681 * v;
    float b = y_full + 1.8556 * u;

    output[id.xy] = float4(clamp(float3(r, g, b), float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), 1.0);
}
