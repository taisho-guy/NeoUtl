struct Uniforms { float4 p[2]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

float sdf_shape(float2 local, int shape, float aspect_ratio)
{
    float2 q = local;
    q.x *= exp(-aspect_ratio * 0.01);
    q.y *= exp(aspect_ratio * 0.01);
    if (shape == 1) {
        float2 b = float2(1.0, 1.0);
        float2 d = abs(q) - b;
        return length(max(d, float2(0.0, 0.0))) + min(max(d.x, d.y), 0.0);
    }
    return length(q) - 1.0;
}

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    int map_shape = int(u.p[0].x);
    float pos_x = u.p[0].y;
    float pos_y = u.p[0].z;
    float rotation = u.p[0].w;
    float size = u.p[1].x;
    float aspect = u.p[1].y;
    float feather = u.p[1].z;
    float scale_amount = u.p[1].w;

    float2 dims = tex_dims(input_tex);
    float2 px = in.uv * dims;
    float2 center = px - float2(pos_x, pos_y) - dims * 0.5;

    float rad = radians(rotation);
    float cs = cos(rad);
    float sn = sin(rad);
    float2 rotated = float2(center.x * cs + center.y * sn, -center.x * sn + center.y * cs);

    float radius = max(size, 0.0001);
    float2 local = rotated / radius;

    float d = sdf_shape(local, map_shape, aspect);
    float feather_n = max(feather / radius, 0.0001);
    float luminance = smoothstep(feather_n, -feather_n, d);
    float variance = (luminance - 0.5) * 2.0;

    float2 radial = length(rotated) > 0.0001 ? normalize(rotated) : float2(0.0, 0.0);
    float2 offset = radial * variance * scale_amount / dims;

    return input_tex.Sample(input_sampler, in.uv + offset);
}
