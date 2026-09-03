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
    fn default() -> Self {
        compute_global_matrix(&Transform::default())
    }
}

pub fn translation_of(m: &GlobalMatrix) -> (f32, f32, f32) {
    (m.0[12], m.0[13], m.0[14])
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
    GlobalMatrix(mat4_mul(&translation, &mat4_mul(&rotation, &scale)))
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
    for i in 0..4 {
        m[i] *= ratio_w;
        m[4 + i] *= ratio_h;
    }
    GlobalMatrix(m)
}

pub fn scale_to_pixels(global: &GlobalMatrix, width_px: f32, height_px: f32) -> GlobalMatrix {
    let mut m = global.0;
    for i in 0..4 {
        m[i] *= width_px;
        m[4 + i] *= height_px;
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

#[derive(Clone, Copy, Debug, PartialEq, Unique)]
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
