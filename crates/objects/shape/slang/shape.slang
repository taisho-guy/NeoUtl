// 標準オブジェクトUniform契約。全ObjectVTable実装が共有するgroup(0)binding(0)レイアウト。
struct Uniforms
{
    float4x4 mvp;
    float opacity;
    float sides;
    float extrude_depth;
    float _pad0;
    float4 fill_color;
};
[[vk::binding(0, 0)]] ConstantBuffer<Uniforms> u;

struct VertexOutput
{
    float4 position : SV_Position;
    float4 color : COLOR0;
};

static const uint MAX_SIDES = 32u;
static const float PI = 3.14159265358979;

// vertex_index: MAX_SIDES個の三角形ファン * (面前+面背)。
// sides未満の三角形は中心点への縮退で不可視化する（固定vertex_countで可変辺数を実現）。
[shader("vertex")]
VertexOutput vs_main(uint in_vertex_index : SV_VertexID)
{
    VertexOutput out;

    uint tri = in_vertex_index / 3u;
    uint corner = in_vertex_index % 3u;
    bool front = tri < MAX_SIDES;
    uint local_tri = front ? tri : tri - MAX_SIDES;

    uint sides_u = max(uint(u.sides), 3u);
    bool visible = local_tri < sides_u;

    float3 pos = float3(0.0, 0.0, 0.0);
    if (visible)
    {
        if (corner == 1u)
        {
            float a = (float(local_tri) / float(sides_u)) * 2.0 * PI;
            pos = float3(cos(a) * 0.5, sin(a) * 0.5, 0.0);
        }
        else if (corner == 2u)
        {
            float a = (float(local_tri + 1u) / float(sides_u)) * 2.0 * PI;
            pos = float3(cos(a) * 0.5, sin(a) * 0.5, 0.0);
        }
    }

    // 背面は押し出し量だけZ方向へ複製する（extrude_depth==0なら2D同一面に潰れ不可視の裏面）。
    if (!front)
    {
        pos.z = -u.extrude_depth;
    }

    out.position = mul(u.mvp, float4(pos, 1.0));
    out.color = float4(u.fill_color.rgb, u.fill_color.a * u.opacity);
    return out;
}

[shader("fragment")]
float4 fs_main(VertexOutput in) : SV_Target
{
    return in.color;
}
