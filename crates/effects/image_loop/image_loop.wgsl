// ImageLoopはクリップのフレームインデックスをホスト側タイムライン評価でループさせる
// 時間軸エフェクトであり、GPUフラグメント処理は入力を素通しする。
struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(input_tex, input_sampler, in.uv);
}
