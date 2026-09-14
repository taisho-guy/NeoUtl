#[derive(Clone, Debug, Default)]
pub struct TrackOutcome {
    pub point_clicked: Option<i32>,
    pub add_point: Option<i32>,
    pub remove_point: Option<i32>,
    pub drag_committed: Option<(i32, i32)>,
}

impl TrackOutcome {
    pub fn empty() -> Self {
        Self::default()
    }
}
