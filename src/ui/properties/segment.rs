use crate::ecs::track::Track;
use crate::ecs::types::Keyframe;

#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub start_frame: i32,
    pub end_frame: i32,
    pub start_value: f32,
    pub end_value: f32,
}

pub fn boundary_frames(track: &[Keyframe]) -> Vec<i32> {
    track.iter().map(|k| k.frame).collect()
}

pub fn resolve_segment(track: &[Keyframe], current_frame: i32, base_value: f32) -> Segment {
    let owned = track.to_vec();
    match (owned.first(), owned.locate(current_frame)) {
        (None, _) => Segment {
            start_frame: current_frame,
            end_frame: current_frame,
            start_value: base_value,
            end_value: base_value,
        },
        (Some(only), None) => Segment {
            start_frame: only.frame,
            end_frame: only.frame,
            start_value: only.value,
            end_value: only.value,
        },
        (Some(_), Some(loc)) => {
            let (a, b) = (&owned[loc.section], &owned[loc.section + 1]);
            Segment {
                start_frame: a.frame,
                end_frame: b.frame,
                start_value: a.value,
                end_value: b.value,
            }
        }
    }
}
