#[derive(Clone, Debug, Default)]
pub struct RowOutcome {
    pub start_value: Option<f32>,
    pub end_value: Option<f32>,
    pub start_commit: bool,
    pub start_release: bool,
    pub end_commit: bool,
    pub end_release: bool,
    pub label_clicked: bool,
}

impl RowOutcome {
    pub fn empty() -> Self {
        Self::default()
    }
}
