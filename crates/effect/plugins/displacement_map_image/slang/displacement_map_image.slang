struct Uniforms { float4 p[2]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;
[[vk::binding(3, 0)]] Texture2D<float4> map_tex;
[[vk::binding(4, 0)]] SamplerState map_sampler;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float pos_x = u.p[0].y;
    float pos_y = u.p[0].z;
    float rotation = u.p[0].w;
    float size = u.p[1].x;
    float aspect = u.p[1].y;
    float disp_x = u.p[1].z;
    float disp_y = u.p[1].w;

    float2 dims = tex_dims(input_tex);
    float2 px = in.uv * dims;
    float2 center = px - float2(pos_x, pos_y) - dims * 0.5;

    float rad = radians(rotation);
    float cs = cos(rad);
    float sn = sin(rad);
    float2 rotated = float2(center.x * cs + center.y * sn, -center.x * sn + center.y * cs);

    float radius = max(size, 0.0001);
    float scale = exp(aspect * 0.01);
    float2 local = float2(rotated.x / (radius * scale), rotated.y * scale / radius);
    float2 map_uv = local * 0.5 + 0.5;

    bool inside = map_uv.x >= 0.0 && map_uv.x <= 1.0 && map_uv.y >= 0.0 && map_uv.y <= 1.0;
    float luminance = 0.5;
    if (inside) {
        float4 mc = map_tex.Sample(map_sampler, map_uv);
        luminance = dot(mc.rgb, float3(0.299, 0.587, 0.114));
    }

    float variance = (luminance - 0.5) * 2.0;
    float2 disp_vec = float2(disp_x, disp_y);
    float2 offset = disp_vec * variance / dims;

    return input_tex.Sample(input_sampler, in.uv + offset);
}
