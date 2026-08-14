use crate::localization::tr;
use crate::ui::types::CatalogRow;
use egui::Ui;

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

        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(t!("検索:"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(t!("エフェクト名を検索…")),
                );
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                for (mode, label) in [(0, "カテゴリ順"), (1, "名前順"), (2, "最近使用")]
                {
                    if ui
                        .selectable_label(self.sort_mode == mode, tr(label))
                        .clicked()
                    {
                        self.sort_mode = mode;
                    }
                }
            });

            ui.add_space(4.0);

            egui::ScrollArea::horizontal()
                .id_salt("category_tabs")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.category_filter.is_empty(), t!("全て"))
                            .clicked()
                        {
                            self.category_filter.clear();
                        }
                        for cat in source.categories() {
                            if ui
                                .selectable_label(&self.category_filter == cat, cat)
                                .clicked()
                            {
                                self.category_filter = cat.clone();
                            }
                        }
                    });
                });

            ui.separator();

            let rows = source.filtered(&self.query, self.sort_mode, &self.category_filter);
            egui::ScrollArea::vertical()
                .id_salt("effect_list_scroll")
                .auto_shrink([false, false])
                .max_height(ui.available_height() - 40.0)
                .show(ui, |ui| {
                    if rows.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x88, 0x88, 0x90),
                            t!("該当するエフェクトがありません"),
                        );
                    }
                    for row in &rows {
                        ui.horizontal(|ui| {
                            let clicked =
                                ui.add(egui::Button::new(&row.name).frame(false)).clicked();
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(0x8a, 0xab, 0xff),
                                        &row.category,
                                    );
                                },
                            );
                            if clicked {
                                confirmed_id = Some(row.id.clone());
                            }
                        });
                    }
                });

            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("閉じる")).clicked() {
                    close_clicked = true;
                }
            });
        });

        if confirmed_id.is_some() || close_clicked {
            self.close();
        }

        confirmed_id
    }
}
