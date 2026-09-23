// OP_WRITE: 変換済みfloat値の出力ターゲットフォーマットへの書き込み。
// 出力はPixelFormat固定集合(Rgba8/Rgba16Float)のみサポート(colorconv.rs対応表と一致)。
// binding番号・フォーマット注釈はcolorconv.rs::P0xxGpuResources::new()の
// BindGroupLayout(StorageTextureAccess::WriteOnly, Rgba8Unorm)と厳密一致させる。
//
// 注意: RWTexture2Dはslangc WGSL出力において書き込み専用用途でも
// read_writeアクセスを要求する(dead-code除去されない既知挙動)。
// read_writeストレージテクスチャはWGSL仕様上r32uint/r32sint/r32float等の
// 単一チャンネル32bit形式のみ許可されRgba8Unormでは使用不可なため、
// neoutl-object-shader-build::compile_compute_wgsl側でコンパイル後の
// WGSLテキストに対しread_write→writeへの後処理置換を行う
// (bit-for-bit同一のRgba8Unormテクスチャへ直接書き込むための対応、
// neo-media-cache側のテクスチャフォーマット変更は不要)。

[[vk::binding(5, 0)]] [format("rgba8")] RWTexture2D<float4> dst_rgba8;
[[vk::binding(6, 0)]] [format("rgba16f")] RWTexture2D<float4> dst_rgba16f;

void op_write_rgba8(int2 coord, float3 rgb, float alpha)
{
    dst_rgba8[coord] = float4(saturate(rgb), alpha);
}

void op_write_rgba16f(int2 coord, float3 rgb, float alpha)
{
    dst_rgba16f[coord] = float4(rgb, alpha);
}
