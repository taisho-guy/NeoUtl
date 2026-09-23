struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float cell = max(u.p[0].x, 1.0);
    float2 dims = tex_dims(input_tex);
    float2 cell_uv = float2(cell, cell) / dims;
    float2 snapped = (floor(in.uv / cell_uv) + float2(0.5, 0.5)) * cell_uv;
    return input_tex.Sample(input_sampler, snapped);
}
