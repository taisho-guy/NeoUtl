// src/ecs/transform.rs
use shipyard::Component;

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

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
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
