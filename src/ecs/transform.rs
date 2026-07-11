// src/ecs/transform.rs
use shipyard::{Component, Unique};

#[derive(Clone, Copy, Debug, Component)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub opacity: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            opacity: 1.0,
        }
    }
}

pub const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

#[derive(Clone, Copy, Debug, Component)]
pub struct GlobalMatrix(pub [f32; 16]);

impl Default for GlobalMatrix {
    fn default() -> Self {
        Self(IDENTITY_MATRIX)
    }
}

pub fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            r[col * 4 + row] = sum;
        }
    }
    r
}

/// Transform を列優先 4x4 行列へ合成する。順序: 平行移動 * 回転Z * 回転Y * 回転X * 拡大縮小。
pub fn compute_global_matrix(t: &Transform) -> GlobalMatrix {
    let translation: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, t.x, t.y, t.z, 1.0,
    ];
    let (sx, cx) = t.rot_x.to_radians().sin_cos();
    let (sy, cy) = t.rot_y.to_radians().sin_cos();
    let (sz, cz) = t.rot_z.to_radians().sin_cos();
    let rot_x: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, cx, sx, 0.0, 0.0, -sx, cx, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let rot_y: [f32; 16] = [
        cy, 0.0, -sy, 0.0, 0.0, 1.0, 0.0, 0.0, sy, 0.0, cy, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let rot_z: [f32; 16] = [
        cz, sz, 0.0, 0.0, -sz, cz, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let scale: [f32; 16] = [
        t.scale_x, 0.0, 0.0, 0.0, 0.0, t.scale_y, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let rotation = mat4_mul(&rot_z, &mat4_mul(&rot_y, &rot_x));
    GlobalMatrix(mat4_mul(&translation, &mat4_mul(&rotation, &scale)))
}

/// 投影方式。2Dシーン既定はOrtho、3Dオブジェクトが1つでも存在すればPerspective選択も可能。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    Ortho,
    Perspective { fov_deg: f32 },
}

#[derive(Clone, Copy, Debug, Unique)]
pub struct Camera {
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub target_z: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 5.0,
            target_x: 0.0,
            target_y: 0.0,
            target_z: 0.0,
            near: 0.01,
            far: 1000.0,
        }
    }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn compute_view_matrix(cam: &Camera) -> [f32; 16] {
    let eye = [cam.pos_x, cam.pos_y, cam.pos_z];
    let target = [cam.target_x, cam.target_y, cam.target_z];
    let up = [0.0f32, 1.0, 0.0];

    let f = normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let s = normalize(cross(f, up));
    let u = cross(s, f);

    [
        s[0],
        u[0],
        -f[0],
        0.0,
        s[1],
        u[1],
        -f[1],
        0.0,
        s[2],
        u[2],
        -f[2],
        0.0,
        -dot(s, eye),
        -dot(u, eye),
        dot(f, eye),
        1.0,
    ]
}

/// 2Dシーン用の正射影。NDC範囲を維持しz奥行きは素通し（深度テストなし前提）。
pub fn compute_ortho_matrix(aspect: f32) -> [f32; 16] {
    let (l, r, b, t) = (-aspect, aspect, -1.0, 1.0);
    [
        2.0 / (r - l),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 / (t - b),
        0.0,
        0.0,
        0.0,
        0.0,
        -1.0,
        0.0,
        -(r + l) / (r - l),
        -(t + b) / (t - b),
        0.0,
        1.0,
    ]
}

pub fn compute_perspective_matrix(fov_deg: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_deg.to_radians() * 0.5).tan();
    let range_inv = 1.0 / (near - far);
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        (near + far) * range_inv,
        -1.0,
        0.0,
        0.0,
        near * far * range_inv * 2.0,
        0.0,
    ]
}

/// dimensionalityとCameraからMVPを合成する唯一の窓口。
pub fn compute_mvp(
    global: &GlobalMatrix,
    cam: &Camera,
    aspect: f32,
    projection: Projection,
) -> [f32; 16] {
    let view = compute_view_matrix(cam);
    let proj = match projection {
        Projection::Ortho => compute_ortho_matrix(aspect),
        Projection::Perspective { fov_deg } => {
            compute_perspective_matrix(fov_deg, aspect, cam.near, cam.far)
        }
    };
    mat4_mul(&proj, &mat4_mul(&view, &global.0))
}
