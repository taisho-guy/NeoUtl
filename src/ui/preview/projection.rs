//! Transform の原点 (ローカル座標 0,0,0) を、レンダラが実際に使用する
//! View×Projection (`compute_mvp`) と同じ式でスクリーン座標へ投影する。
//! 対応範囲: グローバルカメラユニーク1つのみ。レイヤー別カメラ・グループ内カメラ等、
//! `resolve_camera` が解決するカメラ差し替えは対象外
//! (差し替えの解決にはカーテンチェーン全体の再構築が必要なため)。
//! 対応範囲外: オブジェクトの外形（描画サイズ）。原点1点のみを扱う。

use crate::ecs::EcsWorld;
use crate::ecs::transform::{Camera, Projection, compute_global_matrix, compute_mvp};
use neoutl_object_api::UNIT_SIZE_PX;
use shipyard::UniqueView;

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

pub fn project_object_origin(
    world: &EcsWorld,
    object_id: usize,
    image_rect: egui::Rect,
) -> Option<egui::Pos2> {
    let transform = world.get_transform(object_id)?;
    let global = compute_global_matrix(&transform);
    let cam: Camera = world.world.run(|cam: UniqueView<Camera>| *cam);
    let proj = world.get_project();
    let proj_width = resolution_to_f32(proj.width.max(1));
    let proj_height = resolution_to_f32(proj.height.max(1));

    let mvp = compute_mvp(
        &global,
        &cam,
        proj_width,
        proj_height,
        Projection::Perspective {
            fov_deg: cam.fov_deg,
        },
    );
    let clip = transform_point(&mvp, [0.0, 0.0, 0.0, 1.0]);
    if clip[3].abs() < 1e-6 {
        return None;
    }
    let ndc_x = clip[0] / clip[3];
    let ndc_y = clip[1] / clip[3];
    let px = (ndc_x * 0.5 + 0.5) * proj_width;
    let py = (1.0 - (ndc_y * 0.5 + 0.5)) * proj_height;

    let scale = image_rect.width() / proj_width;
    Some(egui::pos2(
        image_rect.left() + px * scale,
        image_rect.top() + py * scale,
    ))
}

/// オブジェクトのソースサイズに基づく4隅をスクリーン座標へ投影する。
/// `project_object_origin` と同じ MVP (`compute_mvp`) を用いるため、
/// 原点投影・4隅投影は常に同一のカメラ・射影設定と整合する。
/// `source_w`/`source_h` は `SourceSizeCache` から取得したピクセル単位のサイズ。
pub fn project_object_corners(
    world: &EcsWorld,
    object_id: usize,
    source_w: f32,
    source_h: f32,
    image_rect: egui::Rect,
) -> Option<[egui::Pos2; 4]> {
    let transform = world.get_transform(object_id)?;
    let global = compute_global_matrix(&transform);
    let cam: Camera = world.world.run(|cam: UniqueView<Camera>| *cam);
    let proj = world.get_project();
    let proj_width = resolution_to_f32(proj.width.max(1));
    let proj_height = resolution_to_f32(proj.height.max(1));

    let mvp = compute_mvp(
        &global,
        &cam,
        proj_width,
        proj_height,
        Projection::Perspective {
            fov_deg: cam.fov_deg,
        },
    );

    let hw = source_w * 0.5 / UNIT_SIZE_PX;
    let hh = source_h * 0.5 / UNIT_SIZE_PX;
    let corners_local = [
        [-hw, hh, 0.0],
        [hw, hh, 0.0],
        [hw, -hh, 0.0],
        [-hw, -hh, 0.0],
    ];

    let mut result = [egui::Pos2::ZERO; 4];
    let scale = image_rect.width() / proj_width;
    for (slot, &[lx, ly, lz]) in result.iter_mut().zip(corners_local.iter()) {
        let clip = transform_point(&mvp, [lx, ly, lz, 1.0]);
        if clip[3].abs() < 1e-6 {
            return None;
        }
        let ndc_x = clip[0] / clip[3];
        let ndc_y = clip[1] / clip[3];
        let px = (ndc_x * 0.5 + 0.5) * proj_width;
        let py = (1.0 - (ndc_y * 0.5 + 0.5)) * proj_height;
        *slot = egui::pos2(
            image_rect.left() + px * scale,
            image_rect.top() + py * scale,
        );
    }
    Some(result)
}
