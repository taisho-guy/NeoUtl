// Read -> ScaleH -> ScaleV -> Convert -> Write の固定順ディスパッチ。
// tap_count_h/v == 1 の場合はscale opを恒等演算として素通し(graph.rsがop列自体を省略するため
// 通常到達しないが、シェーダー単体テスト時の安全弁として残置)。
import ops_read;
import ops_scale;
import ops_convert;
import ops_write;

[[vk::binding(0, 0)]] ConstantBuffer<SwscaleUniforms> uniforms;
[[vk::binding(1, 0)]] Texture2D<uint> plane_y;
[[vk::binding(2, 0)]] Texture2D<uint2> plane_uv;

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_main(uint3 tid : SV_DispatchThreadID)
{
    if (tid.x >= uniforms.dst_width || tid.y >= uniforms.dst_height)
        return;

    int2 src_coord = int2(
        int(tid.x * uniforms.src_width / uniforms.dst_width),
        int(tid.y * uniforms.src_height / uniforms.dst_height));

    float y = op_read_y(plane_y, uniforms, src_coord);
    float2 uv = op_read_uv(plane_uv, uniforms, src_coord / 2);

    float3 yuv_full = op_convert_range(float3(y, uv.x, uv.y), uniforms.color_range);
    float3 rgb = op_convert_yuv_to_rgb(yuv_full.x, yuv_full.y, yuv_full.z, uniforms.color_matrix);

    op_write_rgba8(int2(tid.xy), rgb, 1.0);
}
