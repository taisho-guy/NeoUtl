// 標準オブジェクトUniform契約。全ObjectVTable実装が共有するgroup(0)binding(0)レイアウト。
struct Uniforms {
    mvp: mat4x4<f32>,
    opacity: f32,
    sides: f32,
    extrude_depth: f32,
    _pad0: f32,
    fill_color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

const MAX_SIDES: u32 = 32u;
const PI: f32 = 3.14159265358979;

// vertex_index: MAX_SIDES個の三角形ファン * (面前+面背)。
// sides未満の三角形は中心点への縮退で不可視化する（固定vertex_countで可変辺数を実現）。
@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    let tri = in_vertex_index / 3u;
    let corner = in_vertex_index % 3u;
    let front = tri < MAX_SIDES;
    let local_tri = select(tri - MAX_SIDES, tri, front);

    let sides_u = max(u32(u.sides), 3u);
    let visible = local_tri < sides_u;

    var pos = vec3<f32>(0.0, 0.0, 0.0);
    if (visible) {
        if (corner == 1u) {
            let a = (f32(local_tri) / f32(sides_u)) * 2.0 * PI;
            pos = vec3<f32>(cos(a) * 0.5, sin(a) * 0.5, 0.0);
        } else if (corner == 2u) {
            let a = (f32(local_tri + 1u) / f32(sides_u)) * 2.0 * PI;
            pos = vec3<f32>(cos(a) * 0.5, sin(a) * 0.5, 0.0);
        }
    }

    // 背面は押し出し量だけZ方向へ複製する（extrude_depth==0なら2D同一面に潰れ不可視の裏面）。
    if (!front) {
        pos.z = -u.extrude_depth;
    }

    out.position = u.mvp * vec4<f32>(pos, 1.0);
    out.color = vec4<f32>(u.fill_color.rgb, u.fill_color.a * u.opacity);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
