// src/ecs/resources.rs

/// 動画プロジェクト全体の設定（FPS・解像度等）
#[derive(Clone, Debug)]
pub struct ProjectResource {
    /// フレームレート（frames per second）
    pub fps: u32,
    /// 出力幅（ピクセル）
    pub width: u32,
    /// 出力高さ（ピクセル）
    pub height: u32,
}

impl ProjectResource {
    pub fn new() -> Self {
        Self {
            fps: 30,
            width: 1920,
            height: 1080,
        }
    }

    /// フレーム番号 → 秒数
    pub fn frame_to_seconds(&self, frame: i32) -> f64 {
        frame as f64 / self.fps as f64
    }

    /// 総フレーム数から総再生時間（秒）を計算
    pub fn total_duration_seconds(&self, total_frames: i32) -> f64 {
        total_frames as f64 / self.fps as f64
    }

    /// アスペクト比（width / height）
    pub fn aspect_ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

/// タイムライン状態（再生ヘッド・フレーム総数など）
pub struct TimelineResource {
    pub current_frame: i32,
    pub total_frames: i32,
    pub next_id: usize,
}

impl TimelineResource {
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            total_frames: 300,
            next_id: 1,
        }
    }
}
