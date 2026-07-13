// 契約上のuniformにフレーム/時刻を持たないため、uv位置に基づく空間ワブルとして実装する
// （時間軸アニメーションはホスト側でamplitude/frequencyをキーフレーム評価することで表現する）。
struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let amplitude = u.p[0].x;
    let frequency = u.p[0].y;
    let dims = vec2<f32>(textureDimensions(input_tex));
    let wobble = sin(in.uv.y * frequency * 6.28318) * amplitude / dims.x;
    return textureSample(input_tex, input_sampler, in.uv + vec2<f32>(wobble, 0.0));
}
