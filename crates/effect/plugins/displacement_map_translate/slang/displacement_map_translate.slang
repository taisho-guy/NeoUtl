struct Uniforms { float4 p[3]; };
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
    if (shape == 2) {
        float k = 1.7320508;
        q.y = -q.y;
        q.x = abs(q.x) - 1.0;
        q.y = q.y + 1.0 / k;
        if (q.x + k * q.y > 0.0) { q = float2(q.x - k * q.y, -k * q.x - q.y) / 2.0; }
        q.x -= clamp(q.x, -2.0, 0.0);
        return -length(q) * sign(q.y);
    }
    if (shape == 3 || shape == 4) {
        int n = (shape == 3) ? 5 : 6;
        float an = 3.14159265 / float(n);
        float2 acs = float2(cos(an), sin(an));
        float bn = fmod(atan2(q.x, q.y), 2.0 * an) - an;
        float2 r = float2(cos(bn), abs(sin(bn))) * length(q);
        r -= acs;
        r += acs * clamp(-dot(r, acs), 0.0, acs.x);
        return length(r) * sign(r.x);
    }
    if (shape == 5) {
        float r1 = 1.0;
        float r2 = 0.38;
        int n = 5;
        float an = 3.14159265 / float(n);
        float2 acs = float2(cos(an), sin(an));
        float bn = fmod(atan2(q.x, -q.y), 2.0 * an) - an;
        q = length(q) * float2(cos(bn), abs(sin(bn)));
        q -= r1 * acs;
        q += acs * clamp(-dot(q, acs) + r2 * acs.y / acs.x * (r1 / r2 - 1.0), 0.0, r1 - r2);
        return length(q) * sign(q.x);
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
    float disp_x = u.p[1].w;
    float disp_y = u.p[2].x;

    float2 dims = tex_dims(input_tex);
    float2 px = in.uv * dims;
    float2 center = px - float2(pos_x, pos_y) - dims * 0.5;

    float rad = radians(rotation);
    float cs = cos(rad);
    float sn = sin(rad);
    float2 rotated = float2(center.x * cs + center.y * sn, -center.x * sn + center.y * cs);

    float radius = max(size, 0.0001);
    float2 local = rotated / radius;

    float luminance;
    if (map_shape == 6) {
        luminance = 0.0;
    } else {
        float d = sdf_shape(local, map_shape, aspect);
        float feather_n = max(feather / radius, 0.0001);
        luminance = smoothstep(feather_n, -feather_n, d);
    }

    float variance = (luminance - 0.5) * 2.0;
    float2 disp_vec = float2(disp_x, disp_y);
    float2 offset = disp_vec * variance / dims;

    return input_tex.Sample(input_sampler, in.uv + offset);
}
