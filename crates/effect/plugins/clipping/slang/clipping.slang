struct Uniforms { float4 p[2]; };
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> u;

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    float left = u.p[0].x;
    float top = u.p[0].y;
    float right = u.p[0].z;
    float bottom = u.p[0].w;
    bool invert = u.p[1].x > 0.5;
    bool inside = in.uv.x >= left && in.uv.x <= right && in.uv.y >= top && in.uv.y <= bottom;
    bool visible = invert ? !inside : inside;
    float4 c = input_tex.Sample(input_sampler, in.uv);
    return visible ? c : float4(0.0, 0.0, 0.0, 0.0);
}
