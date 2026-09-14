#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Density;

pub fn density() -> Density {
    Density
}

impl Density {
    pub fn page_margin(self) -> f32 {
        16.0
    }

    pub fn bar_margin_x(self) -> f32 {
        16.0
    }

    pub fn bar_margin_y(self) -> f32 {
        12.0
    }

    pub fn footer_margin(self) -> f32 {
        4.0
    }

    pub fn sidebar_margin_x(self) -> f32 {
        8.0
    }

    pub fn sidebar_margin_y(self) -> f32 {
        12.0
    }

    pub fn section_gap(self) -> f32 {
        12.0
    }

    pub fn sidebar_content_width(self) -> f32 {
        150.0
    }

    pub fn sidebar_panel_width(self) -> f32 {
        150.0 + 16.0
    }
}
