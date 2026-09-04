// OP_SCALE_H / OP_SCALE_V: filters.rs::build_tapsが生成した重みによるタップ畳み込み。
// 重みはops_chain.rs::pack_tap_weightsでstorage bufferへパックされCPU側から供給。
// タップ数が可変のため固定長unrollを避け、動的ループで実装(リスク対策済: レジスタ圧迫回避)。

[[vk::binding(3, 0)]] StructuredBuffer<float> tap_weights_h;
[[vk::binding(4, 0)]] StructuredBuffer<float> tap_weights_v;

float op_scale_h(
    Texture2D<float4> src,
    int2 center,
    uint tap_count,
    int radius)
{
    float4 acc = float4(0, 0, 0, 0);
    for (uint i = 0; i < tap_count; i++)
    {
        int offset = int(i) - radius;
        int2 coord = int2(center.x + offset, center.y);
        acc += src.Load(int3(coord, 0)) * tap_weights_h[i];
    }
    return acc.x; // 呼び出し側でチャネル別に4回呼ぶか、float4版を別途用意
}

float4 op_scale_h4(
    Texture2D<float4> src,
    int2 center,
    uint tap_count,
    int radius)
{
    float4 acc = float4(0, 0, 0, 0);
    for (uint i = 0; i < tap_count; i++)
    {
        int offset = int(i) - radius;
        int2 coord = int2(center.x + offset, center.y);
        acc += src.Load(int3(coord, 0)) * tap_weights_h[i];
    }
    return acc;
}

float4 op_scale_v4(
    Texture2D<float4> src,
    int2 center,
    uint tap_count,
    int radius)
{
    float4 acc = float4(0, 0, 0, 0);
    for (uint i = 0; i < tap_count; i++)
    {
        int offset = int(i) - radius;
        int2 coord = int2(center.x, center.y + offset);
        acc += src.Load(int3(coord, 0)) * tap_weights_v[i];
    }
    return acc;
}
