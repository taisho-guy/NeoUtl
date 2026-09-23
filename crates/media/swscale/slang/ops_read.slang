// OP_READ: 平面テクスチャ→正規化float値ロード。
// storage_bits/bit_depthはSwscaleUniformsから供給(ops_chain.rs::pack_uniforms)。
// NV12(8bit)はshift=0で素通し、P010/P012/P016は上位詰め値をshift補正。

struct SwscaleUniforms
{
    uint color_matrix;
    uint color_range;
    uint bit_depth;
    uint storage_bits;
    uint src_width;
    uint src_height;
    uint dst_width;
    uint dst_height;
    uint tap_count_h;
    uint tap_count_v;
};

uint read_normalize_shift(SwscaleUniforms u)
{
    return u.storage_bits - u.bit_depth;
}

float read_normalize_max(SwscaleUniforms u)
{
    return float((1u << u.bit_depth) - 1u);
}

float op_read_y(Texture2D<uint> plane_y, SwscaleUniforms u, int2 coord)
{
    uint raw = plane_y.Load(int3(coord, 0));
    return float(raw >> read_normalize_shift(u)) / read_normalize_max(u);
}

float2 op_read_uv(Texture2D<uint2> plane_uv, SwscaleUniforms u, int2 coord)
{
    uint2 raw = plane_uv.Load(int3(coord, 0));
    return float2(raw >> read_normalize_shift(u)) / read_normalize_max(u);
}

float4 op_read_rgba8(Texture2D<float4> plane_rgba, int2 coord)
{
    return plane_rgba.Load(int3(coord, 0));
}
