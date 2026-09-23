use egui::{Color32, Frame, Margin, RichText, Ui};
use egui_taffy::taffy;
use egui_taffy::taffy::Style;

pub fn configure_taffy(ctx: &egui::Context) {
    ctx.options_mut(|options| {
        options.max_passes = std::num::NonZeroUsize::new(2).unwrap();
    });
    ctx.all_styles_mut(|style| {
        style.wrap_mode = Some(egui::TextWrapMode::Extend);
    });
}

pub fn row_style(gap: f32) -> Style {
    Style {
        flex_direction: taffy::FlexDirection::Row,
        align_items: Some(taffy::AlignItems::Center),
        gap: taffy::Size {
            width: taffy::LengthPercentage::length(gap),
            height: taffy::LengthPercentage::length(gap),
        },
        ..Default::default()
    }
}

pub fn col_style(gap: f32) -> Style {
    Style {
        flex_direction: taffy::FlexDirection::Column,
        gap: taffy::Size {
            width: taffy::LengthPercentage::length(gap),
            height: taffy::LengthPercentage::length(gap),
        },
        ..Default::default()
    }
}

pub fn row_between_style(width: f32) -> Style {
    Style {
        flex_direction: taffy::FlexDirection::Row,
        justify_content: Some(taffy::JustifyContent::SpaceBetween),
        align_items: Some(taffy::AlignItems::Center),
        size: taffy::Size {
            width: taffy::Dimension::length(width),
            height: taffy::Dimension::auto(),
        },
        ..Default::default()
    }
}

pub fn row_style_full(gap: f32, width: f32) -> Style {
    let mut style = row_style(gap);
    style.size = taffy::Size {
        width: taffy::Dimension::length(width),
        height: taffy::Dimension::auto(),
    };
    style
}

pub fn row_end_style(gap: f32, width: f32) -> Style {
    Style {
        flex_direction: taffy::FlexDirection::Row,
        justify_content: Some(taffy::JustifyContent::End),
        align_items: Some(taffy::AlignItems::Center),
        gap: taffy::Size {
            width: taffy::LengthPercentage::length(gap),
            height: taffy::LengthPercentage::length(gap),
        },
        size: taffy::Size {
            width: taffy::Dimension::length(width),
            height: taffy::Dimension::auto(),
        },
        ..Default::default()
    }
}

pub fn fixed_style(width: f32, height: f32) -> Style {
    Style {
        size: taffy::Size {
            width: taffy::Dimension::length(width),
            height: taffy::Dimension::length(height),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}

pub fn grow_style() -> Style {
    Style {
        flex_grow: 1.0,
        ..Default::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Density;

pub fn density() -> Density {
    Density
}

impl Density {
    fn scale(self) -> f32 {
        1.0
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

    pub fn sidebar_content_width(self) -> f32 {
        150.0 * self.scale()
    }

    pub fn sidebar_panel_width(self) -> f32 {
        let m = self.sidebar_margin();
        self.sidebar_content_width() + f32::from(m.left) + f32::from(m.right)
    }

    pub fn sidebar_frame(self, fill: Color32) -> Frame {
        Frame::default()
            .fill(fill)
            .corner_radius(self.px(10.0) as u8)
            .stroke(egui::Stroke::NONE)
            .inner_margin(self.sidebar_margin())
            .outer_margin(Margin::same(self.px(8.0)))
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
