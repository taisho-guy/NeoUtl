use crate::easings::registry;
use egui::{Rect, Sense, Stroke, Vec2, pos2, vec2};
use neoutl_easing_standard::{CurveKind, curve::evaluate_kind};
use std::sync::OnceLock;

const CARD_MIN_WIDTH: f32 = 84.0;
const CARD_HEIGHT: f32 = 76.0;

struct Preset {
    id: String,
    label: String,
}
struct Group {
    title: &'static str,
    items: Vec<Preset>,
}

const FAMILIES: [(&str, &str); 8] = [
    ("Sine", "サイン"),
    ("Quad", "2次"),
    ("Cubic", "3次"),
    ("Quart", "4次"),
    ("Quint", "5次"),
    ("Expo", "指数"),
    ("Circ", "円"),
    ("Back", "バック"),
];
const VARIANTS: [(&str, &str); 4] = [
    ("In", "イン"),
    ("Out", "アウト"),
    ("InOut", "イン・アウト"),
    ("OutIn", "アウト・イン"),
];

fn builtins() -> &'static [Group] {
    static GROUPS: OnceLock<Vec<Group>> = OnceLock::new();
    GROUPS.get_or_init(|| {
        let make = |family: &str, variants: &[(&str, &str)]| {
            variants
                .iter()
                .map(|(v, jp)| Preset {
                    id: format!("ease{v}{family}"),
                    label: (*jp).to_owned(),
                })
                .collect::<Vec<_>>()
        };
        let mut groups = vec![Group {
            title: "基本",
            items: vec![Preset {
                id: "linear".into(),
                label: "直線".into(),
            }],
        }];
        for (family, jp) in FAMILIES {
            groups.push(Group {
                title: jp,
                items: make(family, &VARIANTS),
            });
        }
        groups.push(Group {
            title: "弾性",
            items: make("Elastic", &VARIANTS[..2]),
        });
        groups.push(Group {
            title: "バウンド",
            items: make("Bounce", &VARIANTS[..2]),
        });
        groups
    })
}

#[derive(Default)]
pub struct PresetUi {
    pub search: String,
    pub focus_search: bool,
    rename: Option<(usize, String)>,
}

impl PresetUi {
    pub fn focused() -> Self {
        Self {
            focus_search: true,
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub struct Out {
    pub apply: Option<CurveKind>,
    pub hover: Option<CurveKind>,
    pub notice: Option<String>,
}

fn default_for(id: &str) -> CurveKind {
    CurveKind::standard(id)
}

fn same_shape(a: &CurveKind, b: &CurveKind) -> bool {
    if matches!(a, CurveKind::Script { .. }) || matches!(b, CurveKind::Script { .. }) {
        return false;
    }
    (0..=8).all(|i| {
        let t = i as f32 / 8.0;
        (evaluate_kind(a, t) - evaluate_kind(b, t)).abs() < 1e-3
    })
}

fn card(
    ui: &mut egui::Ui,
    size: Vec2,
    label: &str,
    kind: &CurveKind,
    active: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let v = ui.visuals().clone();
    let painter = ui.painter();
    let fill = if response.hovered() {
        v.widgets.hovered.bg_fill
    } else {
        v.widgets.inactive.bg_fill
    };
    painter.rect_filled(rect, 4.0, fill);
    if active {
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(2.0, v.selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    let chart = Rect::from_min_max(
        rect.min + vec2(6.0, 6.0),
        pos2(rect.max.x - 6.0, rect.max.y - 20.0),
    );
    painter.rect_stroke(
        chart,
        2.0,
        Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    let line = (0..=32)
        .map(|i| {
            let t = i as f32 / 32.0;
            let y = if matches!(kind, CurveKind::Script { .. }) {
                t
            } else {
                evaluate_kind(kind, t)
            };
            pos2(
                chart.left() + chart.width() * t,
                chart.bottom() - chart.height() * y.clamp(-0.2, 1.2),
            )
        })
        .collect();
    painter
        .with_clip_rect(rect)
        .add(egui::Shape::line(line, Stroke::new(1.5, v.text_color())));
    painter.text(
        pos2(rect.center().x, rect.bottom() - 9.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        v.text_color(),
    );
    response
}

fn grid(
    ui: &mut egui::Ui,
    items: &[(String, String, CurveKind)],
    current: &CurveKind,
    out: &mut Out,
    mut menu: impl FnMut(&mut egui::Ui, usize, &mut Out),
) {
    let gap = ui.spacing().item_spacing.x;
    let avail = super::finite_width(ui);
    let cols = (((avail + gap) / (CARD_MIN_WIDTH + gap)) as usize).clamp(1, 12);
    let width = ((avail - gap * (cols - 1) as f32) / cols as f32).max(CARD_MIN_WIDTH * 0.5);
    for (row, chunk) in items.chunks(cols).enumerate() {
        ui.horizontal(|ui| {
            for (col, (id, label, kind)) in chunk.iter().enumerate() {
                let r = card(
                    ui,
                    vec2(width, CARD_HEIGHT),
                    label,
                    kind,
                    same_shape(kind, current),
                )
                .on_hover_text(id);
                if r.hovered() {
                    out.hover = Some(kind.clone());
                }
                if r.clicked() {
                    out.apply = Some(kind.clone());
                }
                r.context_menu(|ui| menu(ui, row * cols + col, &mut *out));
            }
        });
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PresetUi, current: &CurveKind, out: &mut Out) {
    ui.horizontal(|ui| {
        ui.label(egui_material_icons::icons::ICON_SEARCH);
        let r = ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("名前で検索 (日本語/英語)")
                .desired_width((super::finite_width(ui) - 32.0).max(80.0)),
        );
        if std::mem::take(&mut state.focus_search) {
            r.request_focus();
        }
        if r.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            state.search.clear();
            r.surrender_focus();
        }
        if ui
            .add_enabled(
                !state.search.is_empty(),
                egui::Button::new(egui_material_icons::icons::ICON_CLOSE),
            )
            .on_hover_text("検索をクリア (Esc)")
            .clicked()
        {
            state.search.clear();
        }
    });
    let query = state.search.to_lowercase();
    let hit = |texts: &[&str]| {
        query.is_empty() || texts.iter().any(|t| t.to_lowercase().contains(&query))
    };

    let saved: Vec<(usize, String, CurveKind)> = registry::shared()
        .lock()
        .map(|r| {
            r.entries()
                .iter()
                .enumerate()
                .map(|(i, e)| (i, e.name.clone(), e.kind.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut shown = 0usize;
    egui::ScrollArea::vertical()
        .id_salt("preset_scroll")
        .max_height((ui.ctx().content_rect().height() * 0.6).clamp(240.0, 720.0))
        .show(ui, |ui| {
            let saved_hits: Vec<_> = saved
                .iter()
                .filter(|(_, n, _)| hit(&[n, "保存済み"]))
                .collect();
            shown += saved_hits.len();
            egui::CollapsingHeader::new(format!("保存済み ({})", saved_hits.len()))
                .default_open(true)
                .show(ui, |ui| {
                    if saved_hits.is_empty() {
                        ui.weak("保存済みのカーブはありません。上の「保存」で追加します。");
                    }
                    if let Some((i, text)) = state.rename.as_mut() {
                        let mut done = None;
                        ui.horizontal(|ui| {
                            let edit =
                                ui.add(egui::TextEdit::singleline(text).desired_width(140.0));
                            edit.request_focus();
                            let enter =
                                edit.lost_focus() && ui.input(|s| s.key_pressed(egui::Key::Enter));
                            if ui.button("決定").clicked() || enter {
                                done = Some(true);
                            }
                            if ui.button("取消").clicked()
                                || ui.input(|s| s.key_pressed(egui::Key::Escape))
                            {
                                done = Some(false);
                            }
                        });
                        if let Some(commit) = done {
                            if commit {
                                if let Ok(mut r) = registry::shared().lock() {
                                    r.rename(*i, std::mem::take(text));
                                }
                                out.notice = Some("名前を変更しました".into());
                            }
                            state.rename = None;
                        }
                    }
                    let items: Vec<_> = saved_hits
                        .iter()
                        .map(|(_, n, k)| (n.clone(), n.clone(), k.clone()))
                        .collect();
                    let indices: Vec<usize> = saved_hits.iter().map(|(i, _, _)| *i).collect();
                    grid(ui, &items, current, out, |ui, pos, out| {
                        let Some(&index) = indices.get(pos) else {
                            return;
                        };
                        if ui.button("名前を変更").clicked() {
                            state.rename = Some((index, items_name(&saved, index)));
                            ui.close();
                        }
                        if ui.button("前へ").clicked() {
                            if let Ok(mut r) = registry::shared().lock() {
                                r.move_by(index, -1);
                            }
                            ui.close();
                        }
                        if ui.button("後ろへ").clicked() {
                            if let Ok(mut r) = registry::shared().lock() {
                                r.move_by(index, 1);
                            }
                            ui.close();
                        }
                        if ui.button("削除").clicked() {
                            if let Ok(mut r) = registry::shared().lock() {
                                r.remove(index);
                            }
                            out.notice = Some("削除しました".into());
                            ui.close();
                        }
                    });
                });
            for group in builtins() {
                let items: Vec<_> = group
                    .items
                    .iter()
                    .filter(|p| hit(&[&p.id, &p.label, group.title]))
                    .map(|p| (p.id.clone(), p.label.clone(), default_for(&p.id)))
                    .collect();
                if items.is_empty() {
                    continue;
                }
                shown += items.len();
                egui::CollapsingHeader::new(format!("{} ({})", group.title, items.len()))
                    .default_open(true)
                    .show(ui, |ui| {
                        grid(ui, &items, current, out, |_, _, _| {});
                    });
            }
            if shown == 0 {
                ui.weak("一致するプリセットがありません");
            }
        });
}

fn items_name(saved: &[(usize, String, CurveKind)], index: usize) -> String {
    saved
        .iter()
        .find(|(i, _, _)| *i == index)
        .map(|(_, n, _)| n.clone())
        .unwrap_or_default()
}

pub fn save(kind: CurveKind, state: &mut PresetUi) -> usize {
    let (index, name) = registry::shared()
        .lock()
        .map(|mut r| {
            let i = r.push_unique(kind);
            (i, r.entries()[i].name.clone())
        })
        .unwrap_or_default();
    state.rename = Some((index, name));
    index
}
