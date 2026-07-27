// エフェクトフルスクリーンパス頂点契約。全EffectVTable実装のfs_mainはこのファイルを
// build.rs内でfs_main本体へ前置連結した状態でコンパイルする。
// @group(0)@binding(0)=input_tex, @binding(1)=input_sampler を固定契約とする。
struct VertexOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[[vk::binding(0, 0)]] Texture2D<float4> input_tex;
[[vk::binding(1, 0)]] SamplerState input_sampler;

float2 tex_dims(Texture2D<float4> t)
{
    uint w = 0;
    uint h = 0;
    t.GetDimensions(w, h);
    return float2(float(w), float(h));
}

[shader("vertex")]
VertexOutput vs_main(uint vertex_index : SV_VertexID)
{
    VertexOutput result;
    float x = float((vertex_index << 1u) & 2u);
    float y = float(vertex_index & 2u);
    result.position = float4(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    result.uv = float2(x, y);
    return result;
}
