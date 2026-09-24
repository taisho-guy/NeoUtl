use super::handles::set_boundary;
use neoutl_easing_standard::{CurveKind, add_segment};
use std::ops::RangeInclusive;

#[derive(Default)]
pub struct Edit {
    pub changed: bool,
    pub live: bool,
}

impl Edit {
    pub fn absorb(&mut self, r: &egui::Response) {
        self.changed |= r.changed();
        self.live |= r.dragged();
    }
}

fn field(ui: &mut egui::Ui, edit: &mut Edit, label: &str, v: &mut f32, range: RangeInclusive<f32>) {
    ui.label(label);
    let r = ui.add(
        egui::DragValue::new(v)
            .speed(0.005)
            .range(range)
            .fixed_decimals(3),
    );
    edit.absorb(&r);
}

pub fn show(ui: &mut egui::Ui, kind: &mut CurveKind, edit: &mut Edit) {
    ui.horizontal_wrapped(|ui| match kind {
        CurveKind::Bezier {
            handle_left: l,
            handle_right: r,
        } => {
            field(ui, edit, "x1", &mut l[0], 0.0..=1.0);
            field(ui, edit, "y1", &mut l[1], -2.0..=3.0);
            field(ui, edit, "x2", &mut r[0], 0.0..=1.0);
            field(ui, edit, "y2", &mut r[1], -2.0..=3.0);
        }
        CurveKind::Bounce { cor, period, .. } => {
            field(ui, edit, "反発係数", cor, 0.0..=1.0);
            field(ui, edit, "周期", period, 0.01..=2.0);
        }
        CurveKind::Elastic {
            amplitude,
            frequency,
            decay,
            ..
        } => {
            field(ui, edit, "振幅", amplitude, 0.0..=1.0);
            field(ui, edit, "周波数", frequency, 0.5..=30.0);
            field(ui, edit, "減衰", decay, 0.0..=30.0);
        }
        CurveKind::Normal { segments } => {
            for i in 0..segments.len().saturating_sub(1) {
                let [mut x, mut y] = segments[i].anchor_end;
                ui.label(format!("頂点{}", i + 1));
                let before = edit.changed;
                field(ui, edit, "x", &mut x, 0.0..=1.0);
                field(ui, edit, "y", &mut y, -2.0..=3.0);
                if edit.changed && !before {
                    set_boundary(segments, i, x, y);
                }
            }
            if ui
                .button("頂点追加")
                .on_hover_text("最も広い区間の中央に追加")
                .clicked()
            {
                let (x0, x1) = segments
                    .iter()
                    .map(|s| (s.anchor_start[0], s.anchor_end[0]))
                    .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
                    .unwrap_or((0.0, 1.0));
                add_segment(segments, (x0 + x1) * 0.5);
                edit.changed = true;
            }
        }
        CurveKind::Standard { name } => {
            ui.label(format!("標準イージング: {name} (数値パラメータなし)"));
        }
        CurveKind::Linear => {
            ui.label("直線 (パラメータなし)");
        }
        CurveKind::Script { .. } => {
            ui.label("Lua式で定義 (右のエディタで編集)");
        }
    });
}
