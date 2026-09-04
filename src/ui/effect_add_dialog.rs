use crate::localization::tr;
use crate::ui::types::CatalogRow;
use crate::ui::ui_ext::UiExt;
use egui::Ui;
use elegance::{Button, SegmentedControl, TextInput};

pub trait EffectCatalogSource {
    fn categories(&self) -> &[String];
    fn filtered(&self, query: &str, sort_mode: i32, category: &str) -> Vec<CatalogRow>;
}

pub struct EffectAddDialog {
    pub open: bool,
    query: String,
    sort_mode: i32,
    category_filter: String,
}

impl EffectAddDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            sort_mode: 0,
            category_filter: String::new(),
        }
    }

    pub fn open(&mut self) {
        self.query.clear();
        self.sort_mode = 0;
        self.category_filter.clear();
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn show(&mut self, ui: &mut Ui, source: &dyn EffectCatalogSource) -> Option<String> {
        let mut confirmed_id = None;
        let mut close_clicked = false;

        egui::Panel::bottom("add_effect_footer").show(ui, |ui| {
            ui.footer_bar(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(Button::new(t!("閉じる"))).clicked() {
                        close_clicked = true;
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.page_content(|ui| {
                ui.horizontal(|ui| {
                    ui.label(t!("検索:"));
                    let hint = t!("エフェクト名を検索…").to_string();
                    ui.add(TextInput::new(&mut self.query).hint(hint.as_str()));
                });

                {
                    let sort_labels = ["カテゴリ順", "名前順", "最近使用"];
                    let mut sort_idx = self.sort_mode as usize;
                    ui.add(SegmentedControl::new(
                        &mut sort_idx,
                        sort_labels.iter().map(|l| tr(l)),
                    ));
                    self.sort_mode = sort_idx as i32;
                }

                egui::ScrollArea::horizontal()
                    .id_salt("category_tabs")
                    .show(ui, |ui| {
                        let categories = source.categories();
                        let mut labels: Vec<String> = Vec::with_capacity(categories.len() + 1);
                        labels.push(t!("全て").to_string());
                        labels.extend(categories.iter().cloned());
                        let mut cat_idx = labels
                            .iter()
                            .position(|c| c == &self.category_filter)
                            .unwrap_or(0);
                        ui.add(SegmentedControl::new(&mut cat_idx, labels.iter().cloned()));
                        self.category_filter = if cat_idx == 0 {
                            String::new()
                        } else {
                            labels[cat_idx].clone()
                        };
                    });

                ui.separator();

                let rows = source.filtered(&self.query, self.sort_mode, &self.category_filter);
                egui::ScrollArea::vertical()
                    .id_salt("effect_list_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if rows.is_empty() {
                            ui.colored_label(
                                egui::Color32::from_rgb(0x88, 0x88, 0x90),
                                t!("該当するエフェクトがありません"),
                            );
                        }
                        for row in &rows {
                            let (clicked, _) = egui::Sides::new().shrink_left().truncate().show(
                                ui,
                                |ui| ui.add(Button::new(&row.name).outline()).clicked(),
                                |ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(0x8a, 0xab, 0xff),
                                        &row.category,
                                    )
                                },
                            );
                            if clicked {
                                confirmed_id = Some(row.id.clone());
                            }
                        }
                    });
            });
        });

        if confirmed_id.is_some() || close_clicked {
            self.close();
        }

        confirmed_id
    }
}
