// src/ecs/transform.rs
use crate::ecs::components::ParamAccess;
use neoutl_object_api::UNIT_SIZE_PX;
use serde::{Deserialize, Serialize};
use shipyard::{Component, Unique};

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
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
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Component)]
pub struct GlobalMatrix(pub [f32; 16]);

impl Default for GlobalMatrix {
    /// Transform::default()から必ず導出する。IDENTITY_MATRIX等の別経路の初期値は
    /// 持たない（Transformとの乖離を構造的に不可能にするため）。
    /// エンティティ生成時に本Defaultで挿入されたGlobalMatrixは、
    /// set_transform()を一度も呼ばれていない状態でも常にTransform::default()と
    /// 整合した正しい値になる。
    fn default() -> Self {
        compute_global_matrix(&Transform::default())
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
/// スケール項にはneoutl_object_api::UNIT_SIZE_PX（プラグイン頂点契約の基準サイズ）を
/// 掛け、ローカル単位円（直径1.0）をworld空間のピクセル寸法へ写像する。
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
    GlobalMatrix(mat4_mul(&translation, &mat4_mul(&rotation, &scale)))
}

/// 投影方式。2Dシーン既定はOrtho、3Dオブジェクトが1つでも存在すればPerspective選択も可能。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    Ortho,
    Perspective { fov_deg: f32 },
}

/// 全Perspectiveプロジェクションが共有する既定画角。
/// projection_for()（ecs/systems.rs）とCamera::for_resolution()の両方から参照する
/// 唯一の定義元とし、値の重複・食い違いを防ぐ。
pub const DEFAULT_FOV_DEG: f32 = 45.0;

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

impl Camera {
    /// プロジェクト解像度からカメラを導出する唯一の窓口。
    /// DEFAULT_FOV_DEGのPerspectiveで、z=0平面のオブジェクトが
    /// ちょうどproject_height一杯に収まる距離へpos_zを配置する。
    /// シーン切替・解像度変更のたびにこれを呼び直せば、常に整合したカメラになる。
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
            // near/farはpos_zに対して十分な余裕を持たせ、解像度が変わっても
            // オブジェクトがクリップされないようpos_z基準で比例させる。
            near: (pos_z * 0.01).max(0.1),
            far: (pos_z * 100.0).max(project_width.max(project_height) * 10.0),
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        // ブートストラップ時（EcsWorld::new）専用の暫定値。
        // 実際のプロジェクト解像度が確定次第、apply_scene_resolution経由で
        // Camera::for_resolution()により必ず上書きされる。
        Self::for_resolution(
            crate::ecs::resources::ProjectResource::DEFAULT_WIDTH as f32,
            crate::ecs::resources::ProjectResource::DEFAULT_HEIGHT as f32,
        )
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

/// 2Dシーン用の正射影。world座標=プロジェクトのピクセル座標として扱う
/// （原点中心、+Yを上向き）。深度テストなし前提でz奥行きは素通しする。
pub fn compute_ortho_matrix(project_width: f32, project_height: f32) -> [f32; 16] {
    let (l, r) = (-project_width * 0.5, project_width * 0.5);
    let (b, t) = (-project_height * 0.5, project_height * 0.5);
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
/// project_width/project_height はピクセル単位のプロジェクト解像度で、
/// Orthoではworld座標系そのものの基準として、Perspectiveではaspect算出のみに用いる。
///
/// Orthoは3DカメラのView行列を適用しない。Camera::for_resolutionが導出するpos_zは
/// Perspective専用の値であり、これをOrtho側のz軸へ適用するとclip_zが[0,1]から
/// 大幅に逸脱し、頂点シェーダの時点で全ジオメトリがクリップされ何も描画されなくなる
/// （2Dオブジェクトはz奥行きをそのまま素通しする設計のため、Viewは不要かつ有害）。
pub fn compute_mvp(
    global: &GlobalMatrix,
    cam: &Camera,
    project_width: f32,
    project_height: f32,
    projection: Projection,
) -> [f32; 16] {
    match projection {
        Projection::Ortho => {
            let proj = compute_ortho_matrix(project_width, project_height);
            mat4_mul(&proj, &global.0)
        }
        Projection::Perspective { fov_deg } => {
            let view = compute_view_matrix(cam);
            let aspect = project_width.max(1.0) / project_height.max(1.0);
            let proj = compute_perspective_matrix(fov_deg, aspect, cam.near, cam.far);
            mat4_mul(&proj, &mat4_mul(&view, &global.0))
        }
    }
}
