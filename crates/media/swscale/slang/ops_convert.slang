// OP_CONVERT: YUV<->RGB色行列変換 + Limited/Fullレンジ正規化。
// csputils.rs::matrix_coeffs と同一係数をuniform索引(color_matrix)経由で参照。

float3 op_convert_matrix_coeffs(uint color_matrix)
{
    if (color_matrix == 0) return float3(0.299, 0.114, 0.0);   // BT601: kr, kb
    if (color_matrix == 2) return float3(0.2627, 0.0593, 0.0); // BT2020
    return float3(0.2126, 0.0722, 0.0);                        // BT709
}

float3 op_convert_range(float3 yuv, uint color_range)
{
    // color_range: 0=Limited(16-235相当を既にRead段で0-1正規化済のためスケールのみ), 1=Full
    if (color_range == 0)
    {
        yuv.x = (yuv.x - 16.0 / 255.0) * (255.0 / 219.0);
        yuv.yz = (yuv.yz - 128.0 / 255.0) * (255.0 / 224.0) + 0.5;
    }
    return yuv;
}

float3 op_convert_yuv_to_rgb(float y, float u, float v, uint color_matrix)
{
    float3 kk = op_convert_matrix_coeffs(color_matrix);
    float kr = kk.x;
    float kb = kk.y;
    float kg = 1.0 - kr - kb;
    float uu = u - 0.5;
    float vv = v - 0.5;
    float r = y + 2.0 * (1.0 - kr) * vv;
    float b = y + 2.0 * (1.0 - kb) * uu;
    float g = (y - kr * r - kb * b) / kg;
    return float3(r, g, b);
}

float3 op_convert_rgb_to_yuv(float3 rgb, uint color_matrix)
{
    float3 kk = op_convert_matrix_coeffs(color_matrix);
    float kr = kk.x;
    float kb = kk.y;
    float kg = 1.0 - kr - kb;
    float y = kr * rgb.r + kg * rgb.g + kb * rgb.b;
    float u = (rgb.b - y) / (2.0 * (1.0 - kb)) + 0.5;
    float v = (rgb.r - y) / (2.0 * (1.0 - kr)) + 0.5;
    return float3(y, u, v);
}
