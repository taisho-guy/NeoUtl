use crate::ecs::components::ParamAccess;
use neoutl_object_api::UNIT_SIZE_PX;
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
    pub center_x: f32,
    pub center_y: f32,
    pub center_z: f32,
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
            center_x: 0.0,
            center_y: 0.0,
            center_z: 0.0,
        }
    }
}

impl ParamAccess for Transform {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "x" => self.x,
            "y" => self.y,
            "z" => self.z,
            "scale_x" => self.scale_x,
            "scale_y" => self.scale_y,
            "rot_x" => self.rot_x,
            "rot_y" => self.rot_y,
            "rot_z" => self.rot_z,
            "opacity" => self.opacity,
            "center_x" => self.center_x,
            "center_y" => self.center_y,
            "center_z" => self.center_z,
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "x" => self.x = value,
            "y" => self.y = value,
            "z" => self.z = value,
            "scale_x" => self.scale_x = value,
            "scale_y" => self.scale_y = value,
            "rot_x" => self.rot_x = value,
            "rot_y" => self.rot_y = value,
            "rot_z" => self.rot_z = value,
            "opacity" => self.opacity = value,
            "center_x" => self.center_x = value,
            "center_y" => self.center_y = value,
            "center_z" => self.center_z = value,
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Component)]
pub struct GlobalMatrix(pub [f32; 16]);

impl Default for GlobalMatrix {
    fn default() -> Self {
        compute_global_matrix(&Transform::default())
    }
}

pub fn translation_of(m: &GlobalMatrix) -> (f32, f32, f32) {
    let mut translation = m.0.iter().skip(12).take(3).copied();
    (
        translation.next().unwrap_or_default(),
        translation.next().unwrap_or_default(),
        translation.next().unwrap_or_default(),
    )
}

pub fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for (b_column, output_column) in b
        .as_chunks::<4>()
        .0
        .iter()
        .zip(r.as_chunks_mut::<4>().0.iter_mut())
    {
        for (row, slot) in output_column.iter_mut().enumerate() {
            *slot = a
                .as_chunks::<4>()
                .0
                .iter()
                .zip(b_column.iter())
                .map(|(a_column, b_value)| a_column.get(row).copied().unwrap_or_default() * b_value)
                .sum();
        }
    }
    r
}

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
        t.scale_x * UNIT_SIZE_PX,
        0.0,
        0.0,
        0.0,
        0.0,
        t.scale_y * UNIT_SIZE_PX,
        0.0,
        0.0,
        0.0,
        0.0,
        UNIT_SIZE_PX,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ];
    let rotation = mat4_mul(&rot_z, &mat4_mul(&rot_y, &rot_x));
    let center_to: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, t.center_x, t.center_y,
        t.center_z, 1.0,
    ];
    let center_from: [f32; 16] = [
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        -t.center_x,
        -t.center_y,
        -t.center_z,
        1.0,
    ];
    GlobalMatrix(mat4_mul(
        &translation,
        &mat4_mul(
            &center_to,
            &mat4_mul(&rotation, &mat4_mul(&scale, &center_from)),
        ),
    ))
}

impl From<&Transform> for neoutl_schema::Transform {
    fn from(value: &Transform) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
            scale_x: value.scale_x,
            scale_y: value.scale_y,
            rot_x: value.rot_x,
            rot_y: value.rot_y,
            rot_z: value.rot_z,
            opacity: value.opacity,
            center_x: value.center_x,
            center_y: value.center_y,
            center_z: value.center_z,
        }
    }
}

impl TryFrom<&neoutl_schema::Transform> for Transform {
    type Error = String;

    fn try_from(value: &neoutl_schema::Transform) -> Result<Self, Self::Error> {
        Ok(Self {
            x: value.x,
            y: value.y,
            z: value.z,
            scale_x: value.scale_x,
            scale_y: value.scale_y,
            rot_x: value.rot_x,
            rot_y: value.rot_y,
            rot_z: value.rot_z,
            opacity: value.opacity,
            center_x: value.center_x,
            center_y: value.center_y,
            center_z: value.center_z,
        })
    }
}

pub fn compute_relative_matrix(t: &Transform) -> GlobalMatrix {
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
    let center_to: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, t.center_x, t.center_y,
        t.center_z, 1.0,
    ];
    let center_from: [f32; 16] = [
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        -t.center_x,
        -t.center_y,
        -t.center_z,
        1.0,
    ];
    GlobalMatrix(mat4_mul(
        &translation,
        &mat4_mul(
            &center_to,
            &mat4_mul(&rotation, &mat4_mul(&scale, &center_from)),
        ),
    ))
}

/// 4x4逆行列 (余因子展開によるアジュゲート法)。
/// 添字は `mat4_mul` と同じ列優先 (`col * 4 + row`)。
/// 行列式が0に近い場合 (特異行列: 非可逆な射影設定等) は None を返す。
/// 呼び出し側は None を「逆投影不能」として扱い、既定カメラ前提の近似へフォールバックする。
pub fn mat4_inverse(m: &[f32; 16]) -> Option<[f32; 16]> {
    let mut inv = [0.0f32; 16];

    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];

    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];

    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];

    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-6 {
        return None;
    }
    let inv_det = 1.0 / det;
    let mut out = [0.0f32; 16];
    for (o, i) in out.iter_mut().zip(inv.iter()) {
        *o = i * inv_det;
    }
    Some(out)
}

pub fn compute_chained_matrix(curtains: &[GlobalMatrix], leaf: &GlobalMatrix) -> GlobalMatrix {
    curtains.iter().rev().fold(*leaf, |acc, curtain| {
        GlobalMatrix(mat4_mul(&curtain.0, &acc.0))
    })
}

pub fn rescale_for_source(global: &GlobalMatrix, source_w: f32, source_h: f32) -> GlobalMatrix {
    let mut m = global.0;
    let ratio_w = source_w / UNIT_SIZE_PX;
    let ratio_h = source_h / UNIT_SIZE_PX;
    for value in m.iter_mut().take(4) {
        *value *= ratio_w;
    }
    for value in m.iter_mut().skip(4).take(4) {
        *value *= ratio_h;
    }
    GlobalMatrix(m)
}

pub fn scale_to_pixels(global: &GlobalMatrix, width_px: f32, height_px: f32) -> GlobalMatrix {
    let mut m = global.0;
    for value in m.iter_mut().take(4) {
        *value *= width_px;
    }
    for value in m.iter_mut().skip(4).take(4) {
        *value *= height_px;
    }
    GlobalMatrix(m)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    Perspective { fov_deg: f32 },
}

pub const DEFAULT_FOV_DEG: f32 = 45.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TargetLayerMode {
    Origin,
    CameraRelative,
    Layer(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Component, Unique)]
pub struct Camera {
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub target_z: f32,
    pub near: f32,
    pub far: f32,
    pub tilt_deg: f32,
    pub fov_deg: f32,
    pub target_layer_mode: TargetLayerMode,
    pub zbuffer_enabled: bool,
    pub focus_distance: f32,
    pub depth_blur_strength: f32,
}

impl Camera {
    pub fn for_resolution(project_width: f32, project_height: f32) -> Self {
        let half_fov = (DEFAULT_FOV_DEG * 0.5).to_radians();
        let pos_z = (project_height.max(1.0) * 0.5) / half_fov.tan();
        Self {
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z,
            target_x: 0.0,
            target_y: 0.0,
            target_z: 0.0,
            near: (pos_z * 0.01).max(0.1),
            far: (pos_z * 100.0).max(project_width.max(project_height) * 10.0),
            tilt_deg: 0.0,
            fov_deg: DEFAULT_FOV_DEG,
            target_layer_mode: TargetLayerMode::Origin,
            zbuffer_enabled: false,
            focus_distance: pos_z,
            depth_blur_strength: 0.0,
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::for_resolution(
            u32_to_f32(crate::ecs::resources::ProjectResource::DEFAULT_WIDTH),
            u32_to_f32(crate::ecs::resources::ProjectResource::DEFAULT_HEIGHT),
        )
    }
}

fn u32_to_f32(value: u32) -> f32 {
    value.to_string().parse().unwrap_or(f32::MAX)
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
    let s0 = normalize(cross(f, up));
    let u0 = cross(s0, f);

    let (st, ct) = cam.tilt_deg.to_radians().sin_cos();
    let s = [
        s0[0] * ct + u0[0] * st,
        s0[1] * ct + u0[1] * st,
        s0[2] * ct + u0[2] * st,
    ];
    let u = [
        u0[0] * ct - s0[0] * st,
        u0[1] * ct - s0[1] * st,
        u0[2] * ct - s0[2] * st,
    ];

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
        far * range_inv,
        -1.0,
        0.0,
        0.0,
        near * far * range_inv,
        0.0,
    ]
}

pub fn view_space_depth(global: &GlobalMatrix, cam: &Camera) -> f32 {
    let (x, y, z) = translation_of(global);
    let view = compute_view_matrix(cam);
    -(view[2] * x + view[6] * y + view[10] * z + view[14])
}

pub fn compute_mvp(
    global: &GlobalMatrix,
    cam: &Camera,
    project_width: f32,
    project_height: f32,
    projection: Projection,
) -> [f32; 16] {
    match projection {
        Projection::Perspective { fov_deg } => {
            let view = compute_view_matrix(cam);
            let aspect = project_width.max(1.0) / project_height.max(1.0);
            let proj = compute_perspective_matrix(fov_deg, aspect, cam.near, cam.far);
            mat4_mul(&proj, &mat4_mul(&view, &global.0))
        }
    }
}

#[cfg(test)]
mod mat4_inverse_tests {
    use super::*;

    fn mat4_identity() -> [f32; 16] {
        let mut m = [0.0f32; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        m
    }

    fn approx_eq_mat4(a: &[f32; 16], b: &[f32; 16], eps: f32) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < eps)
    }

    #[test]
    fn inverse_of_identity_is_identity() {
        let id = mat4_identity();
        let inv = mat4_inverse(&id).expect("identity must be invertible");
        assert!(approx_eq_mat4(&inv, &id, 1e-6));
    }

    #[test]
    fn inverse_round_trip_on_general_matrix() {
        let t = Transform {
            x: 3.0,
            y: -2.0,
            z: 5.0,
            scale_x: 1.5,
            scale_y: 0.8,
            rot_x: 12.0,
            rot_y: 33.0,
            rot_z: 7.0,
            opacity: 1.0,
            center_x: 0.4,
            center_y: -0.6,
            center_z: 0.0,
        };
        let m = compute_global_matrix(&t).0;
        let inv = mat4_inverse(&m).expect("non-degenerate transform must be invertible");
        let round_trip = mat4_mul(&m, &inv);
        assert!(approx_eq_mat4(&round_trip, &mat4_identity(), 1e-3));
    }

    #[test]
    fn singular_matrix_returns_none() {
        let zero = [0.0f32; 16];
        assert!(mat4_inverse(&zero).is_none());
    }
}
