//! `properties.slint`のコメント通り、区間探索（現在フレームを含む境界区間の確定）は
//! Rust側のみで行い、UI側（row.rs/track.rs）は確定済みSegmentを描画・編集する専用層に留める。

use crate::ecs::types::Keyframe;

#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub start_frame: i32,
    pub end_frame: i32,
    pub start_value: f32,
    pub end_value: f32,
}

/// KeyframeTrack.boundary-frames相当。先頭=clip_start、末尾=clip_endを常に含む。
pub fn boundary_frames(track: &[Keyframe], clip_start: i32, clip_end: i32) -> Vec<i32> {
    let mut frames: Vec<i32> = track.iter().map(|k| k.frame).collect();
    frames.push(clip_start);
    frames.push(clip_end);
    frames.sort_unstable();
    frames.dedup();
    frames
}

/// frameに実キーフレームが無い場合、直前キーフレームの値を保持する（ホールド補間）。
/// 直前が無ければ直後、直後も無ければbase_value（非キーフレーム時の静的値）を返す。
fn value_at(track: &[Keyframe], frame: i32, base_value: f32) -> f32 {
    if let Some(k) = track.iter().find(|k| k.frame == frame) {
        return k.value;
    }
    if let Some(k) = track
        .iter()
        .filter(|k| k.frame < frame)
        .max_by_key(|k| k.frame)
    {
        return k.value;
    }
    if let Some(k) = track
        .iter()
        .filter(|k| k.frame > frame)
        .min_by_key(|k| k.frame)
    {
        return k.value;
    }
    base_value
}

/// current_frameを含む区間の境界フレーム・値を確定する。
/// 戻り値のstart_frame/end_frameが、PropertyRow左右セグメントの書き込み先フレームとなる。
/// clip_start == clip_end (新規シーン等、尺0のクリップ) の場合は境界が1点のみとなり、
/// start/end とも同一フレームの区間として扱う（境界外参照によるパニックを防ぐ）。
pub fn resolve_segment(
    track: &[Keyframe],
    clip_start: i32,
    clip_end: i32,
    current_frame: i32,
    base_value: f32,
) -> Segment {
    let bounds = boundary_frames(track, clip_start, clip_end);
    let last_idx = bounds.len() - 1;
    let frame = current_frame.clamp(clip_start, clip_end);
    let idx = match bounds.binary_search(&frame) {
        Ok(i) => i.min(last_idx.saturating_sub(1)),
        Err(i) => i.saturating_sub(1).min(last_idx.saturating_sub(1)),
    };
    let start_frame = bounds[idx];
    let end_frame = bounds[(idx + 1).min(last_idx)];
    Segment {
        start_frame,
        end_frame,
        start_value: value_at(track, start_frame, base_value),
        end_value: value_at(track, end_frame, base_value),
    }
}
