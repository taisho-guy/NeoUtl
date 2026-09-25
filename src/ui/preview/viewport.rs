//! スクリーン座標のデルタをシーン座標 (z=0平面) のデルタへ変換する。
//! カメラのView-Projection逆行列を用いてレイキャスト (アンプロジェクト) するため、
//! `Camera::for_resolution` の既定カメラに限らず、任意のカメラ設定 (位置・tilt・fov) で正確に成立する。
//! 逆行列が求まらない場合 (射影が特異になる極端な near/far/fov 設定等) は、
//! 既定カメラを前提とした等倍アフィン変換にフォールバックする。
//! ドラッグ開始時に対象オブジェクトのカーテンチェーンから解決したカメラを受け取り、
//! そのカメラに対してスクリーン座標をシーン座標へ変換する。

use crate::ecs::transform::{
    Camera, compute_perspective_matrix, compute_view_matrix, mat4_inverse, mat4_mul,
};
use std::ops::Add;

fn transform_point(m: &[f32; 16], p: [f32; 4]) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    let first = m.iter().take(4);
    let second = m.iter().skip(4).take(4);
    let third = m.iter().skip(8).take(4);
    let fourth = m.iter().skip(12).take(4);
    for (slot, (((m0, m1), m2), m3)) in out.iter_mut().zip(first.zip(second).zip(third).zip(fourth))
    {
        *slot = m0.mul_add(p[0], m1.mul_add(p[1], m2.mul_add(p[2], m3 * p[3])));
    }
    out
}

fn resolution_to_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

#[derive(Clone, Copy, Debug)]
pub struct ViewportState {
    image_rect: egui::Rect,
    scene_width: f32,
    scene_height: f32,
    inv_vp: Option<[f32; 16]>,
}

impl ViewportState {
    pub fn new(
        image_rect: egui::Rect,
        scene_width: u32,
        scene_height: u32,
        camera: &Camera,
    ) -> Self {
        let sw = resolution_to_f32(scene_width.max(1));
        let sh = resolution_to_f32(scene_height.max(1));
        let view = compute_view_matrix(camera);
        let aspect = sw / sh;
        let proj = compute_perspective_matrix(camera.fov_deg, aspect, camera.near, camera.far);
        let vp = mat4_mul(&proj, &view);
        Self {
            image_rect,
            scene_width: sw,
            scene_height: sh,
            inv_vp: mat4_inverse(&vp),
        }
    }

    fn scale(&self) -> f32 {
        self.scene_width / self.image_rect.width().max(1.0)
    }

    /// スクリーン座標を z=0 平面上のシーン座標へアンプロジェクトする。
    /// NDC上の同一(x,y)についてnear/far2点をワールド空間へ逆投影し、
    /// z=0平面とのレイ交点を求める (レイキャスト)。
    fn unproject_to_z0(&self, screen_pos: egui::Pos2) -> Option<(f32, f32)> {
        let inv_vp = self.inv_vp?;
        let scale = self.scale();
        let px = (screen_pos.x - self.image_rect.left()) * scale;
        let py = (screen_pos.y - self.image_rect.top()) * scale;
        let ndc_x = (px / self.scene_width) * 2.0 - 1.0;
        let ndc_y = 1.0 - (py / self.scene_height) * 2.0;

        let near_clip = transform_point(&inv_vp, [ndc_x, ndc_y, -1.0, 1.0]);
        let far_clip = transform_point(&inv_vp, [ndc_x, ndc_y, 1.0, 1.0]);
        if near_clip[3].abs() < 1e-6 || far_clip[3].abs() < 1e-6 {
            return None;
        }
        let near = [
            near_clip[0] / near_clip[3],
            near_clip[1] / near_clip[3],
            near_clip[2] / near_clip[3],
        ];
        let far = [
            far_clip[0] / far_clip[3],
            far_clip[1] / far_clip[3],
            far_clip[2] / far_clip[3],
        ];
        let dz = far[2] - near[2];
        if dz.abs() < 1e-6 {
            return None;
        }
        let t = -near[2] / dz;
        Some((
            near[0] + t * (far[0] - near[0]),
            near[1] + t * (far[1] - near[1]),
        ))
    }

    /// スクリーン上の移動量をシーン座標系の移動量へ変換する。
    /// スクリーンYは下方向が正、シーンYは上方向が正のため符号を反転する
    /// (フォールバック経路のみ。逆行列経路は行列自体がこの反転を含む)。
    pub fn screen_delta_to_scene(&self, delta: egui::Vec2) -> (f32, f32) {
        let center = self.image_rect.center();
        if let (Some((x0, y0)), Some((x1, y1))) = (
            self.unproject_to_z0(center),
            self.unproject_to_z0(egui::pos2(center.x.add(delta.x), center.y.add(delta.y))),
        ) {
            return (x1 - x0, y1 - y0);
        }
        let scale = self.scale();
        (delta.x * scale, -delta.y * scale)
    }
}
