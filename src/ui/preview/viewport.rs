//! 対応範囲: `Camera::for_resolution` が生成する既定カメラ
//! (位置固定・tilt_deg=0・target=原点) のみ。
//! この条件下では z=0 平面の可視幅・可視高がプロジェクト解像度と一致するため、
//! スクリーン座標とシーン座標の変換は等倍のアフィン変換になる。
//! カメラオブジェクトが有効化されているレイヤーでは本変換は成立しない。

#[derive(Clone, Copy, Debug)]
pub struct ViewportState {
    image_rect: egui::Rect,
    scene_width: f32,
    scene_height: f32,
}

impl ViewportState {
    pub fn new(image_rect: egui::Rect, scene_width: u32, scene_height: u32) -> Self {
        Self {
            image_rect,
            scene_width: scene_width.max(1) as f32,
            scene_height: scene_height.max(1) as f32,
        }
    }

    fn scale(&self) -> f32 {
        self.scene_width / self.image_rect.width().max(1.0)
    }

    /// スクリーン上の移動量をシーン座標系の移動量へ変換する。
    /// スクリーンYは下方向が正、シーンYは上方向が正のため符号を反転する。
    pub fn screen_delta_to_scene(&self, delta: egui::Vec2) -> (f32, f32) {
        let scale = self.scale();
        (delta.x * scale, -delta.y * scale)
    }
}
