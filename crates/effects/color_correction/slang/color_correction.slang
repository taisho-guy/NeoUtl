struct Uniforms { float4 p[1]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

float3 rgb_to_hsv(float3 c)
{
    float maxc = max(c.r, max(c.g, c.b));
    float minc = min(c.r, min(c.g, c.b));
    float v = maxc;
    float delta = maxc - minc;
    float h = 0.0;
    float s = 0.0;
    if (maxc > 0.0) { s = delta / maxc; }
    if (delta > 0.0001)
    {
        if (maxc == c.r) { h = (c.g - c.b) / delta; }
        else if (maxc == c.g) { h = 2.0 + (c.b - c.r) / delta; }
        else { h = 4.0 + (c.r - c.g) / delta; }
        h = h / 6.0;
        if (h < 0.0) { h += 1.0; }
    }
    return float3(h, s, v);
}

float3 hsv_to_rgb(float3 c)
{
    float h = c.x * 6.0;
    float s = c.y;
    float v = c.z;
    float i = floor(h);
    float f = h - i;
    float p = v * (1.0 - s);
    float q = v * (1.0 - s * f);
    float t = v * (1.0 - s * (1.0 - f));
    int m = int(i) % 6;
    if (m == 0) { return float3(v, t, p); }
    if (m == 1) { return float3(q, v, p); }
    if (m == 2) { return float3(p, v, t); }
    if (m == 3) { return float3(p, q, v); }
    if (m == 4) { return float3(t, p, v); }
    return float3(v, p, q);
}

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float brightness = u.p[0].x;
    float contrast = u.p[0].y;
    float saturation = u.p[0].z;
    float hue = u.p[0].w;
    float4 c = input_tex.Sample(input_sampler, in.uv);
    float3 hsv = rgb_to_hsv(c.rgb);
    hsv.x = frac(hsv.x + hue / 360.0);
    hsv.y = clamp(hsv.y * (1.0 + saturation), 0.0, 1.0);
    float3 rgb = hsv_to_rgb(hsv);
    rgb = (rgb - float3(0.5, 0.5, 0.5)) * (1.0 + contrast) + float3(0.5, 0.5, 0.5) + float3(brightness, brightness, brightness);
    return float4(clamp(rgb, float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), c.a);
}
