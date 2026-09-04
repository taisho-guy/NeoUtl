use egui::{Color32, Frame, Margin, RichText, Ui};
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

static DENSITY: Mutex<Density> = Mutex::new(Density::Comfortable);

pub fn density() -> Density {
    *DENSITY.lock().unwrap()
}

pub fn set_density(value: Density) {
    *DENSITY.lock().unwrap() = value;
}

impl Density {
    fn scale(self) -> f32 {
        match self {
            Density::Comfortable => 1.0,
            Density::Compact => 0.7,
        }
    }

    fn px(self, base: f32) -> i8 {
        (base * self.scale()).round() as i8
    }

    pub fn page_margin(self) -> Margin {
        Margin::same(self.px(16.0))
    }

    pub fn bar_margin(self) -> Margin {
        Margin::symmetric(self.px(16.0), self.px(12.0))
    }

    pub fn footer_margin(self) -> Margin {
        Margin::same(self.px(4.0))
    }

    pub fn sidebar_margin(self) -> Margin {
        Margin::symmetric(self.px(8.0), self.px(12.0))
    }

    pub fn section_gap(self) -> f32 {
        12.0 * self.scale()
    }

    pub fn sidebar_width(self) -> f32 {
        150.0 * self.scale()
    }
}

#[inline]
pub fn section_heading_color(ui: &Ui) -> Color32 {
    ui.visuals().hyperlink_color
}

#[inline]
pub fn page_title(text: impl Into<String>) -> RichText {
    RichText::new(text).heading().strong()
}

pub trait UiExt {
    fn page_content<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R;

    fn header_bar<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R;

    fn footer_bar<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R;

    fn sidebar<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R;

    fn section<R>(
        &mut self,
        heading: impl Into<String>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> R;
}

impl UiExt for Ui {
    fn page_content<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        Frame::default()
            .inner_margin(density().page_margin())
            .show(self, add_contents)
            .inner
    }

    fn header_bar<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        Frame::default()
            .inner_margin(density().bar_margin())
            .show(self, add_contents)
            .inner
    }

    fn footer_bar<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        Frame::default()
            .inner_margin(density().footer_margin())
            .show(self, add_contents)
            .inner
    }

    fn sidebar<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        let bg = self.visuals().faint_bg_color;
        let width = density().sidebar_width();
        Frame::default()
            .fill(bg)
            .inner_margin(density().sidebar_margin())
            .show(self, |ui| {
                ui.set_width(width);
                add_contents(ui)
            })
            .inner
    }

    fn section<R>(
        &mut self,
        heading: impl Into<String>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let color = section_heading_color(self);
        self.label(RichText::new(heading.into()).strong().color(color));
        let result = add_contents(self);
        self.add_space(density().section_gap());
        result
    }
}
