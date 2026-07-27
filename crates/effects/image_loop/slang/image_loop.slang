// ImageLoopはクリップのフレームインデックスをホスト側タイムライン評価でループさせる
// 時間軸エフェクトであり、GPUフラグメント処理は入力を素通しする。
struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    return input_tex.Sample(input_sampler, in.uv);
}
