// 契約上のuniformにフレーム/時刻を持たないため、uv位置に基づく空間ワブルとして実装する
// （時間軸アニメーションはホスト側でamplitude/frequencyをキーフレーム評価することで表現する）。
struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float amplitude = u.p[0].x;
    float frequency = u.p[0].y;
    float2 dims = tex_dims(input_tex);
    float wobble = sin(in.uv.y * frequency * 6.28318) * amplitude / dims.x;
    return input_tex.Sample(input_sampler, in.uv + float2(wobble, 0.0));
}
