struct Uniforms { float4 p[2]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    int shape = int(u.p[0].x + 0.5);
    float radius = u.p[0].y;
    float feather = u.p[0].z;
    bool invert = u.p[0].w > 0.5;
    bool fit_to_size = u.p[1].x > 0.5;

    float2 center = float2(0.5, 0.5);
    float2 d = in.uv - center;
    float sd;
    if (shape == 0) {
        sd = length(d) - radius;
    } else {
        float2 half_size = fit_to_size ? float2(radius, radius) : float2(radius, radius * 0.5625);
        float2 q = abs(d) - half_size;
        sd = length(max(q, float2(0.0, 0.0))) + min(max(q.x, q.y), 0.0);
    }

    float a = 1.0 - smoothstep(0.0, max(feather, 0.0001), sd);
    if (invert) {
        a = 1.0 - a;
    }
    float4 c = input_tex.Sample(input_sampler, in.uv);
    return float4(c.rgb, c.a * a);
}
