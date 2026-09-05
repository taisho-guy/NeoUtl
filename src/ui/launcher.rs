use crate::project::{self, ProjectMeta};
use elegance::{Accent, Button, Card, Checkbox, TextInput, Theme};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortKey {
    NameAsc,
    NameDesc,
    DateAsc,
    DateDesc,
}

impl SortKey {
    fn next(self) -> Self {
        match self {
            SortKey::NameAsc => SortKey::NameDesc,
            SortKey::NameDesc => SortKey::DateAsc,
            SortKey::DateAsc => SortKey::DateDesc,
            SortKey::DateDesc => SortKey::NameAsc,
        }
    }

    fn label(self) -> String {
        match self {
            SortKey::NameAsc => t!("並べ替え：名前 ↑"),
            SortKey::NameDesc => t!("並べ替え：名前 ↓"),
            SortKey::DateAsc => t!("並べ替え：更新日時 ↑"),
            SortKey::DateDesc => t!("並べ替え：更新日時 ↓"),
        }
    }

    fn apply(self, list: &mut [ProjectMeta]) {
        match self {
            SortKey::NameAsc => list.sort_by(|a, b| a.name.cmp(&b.name)),
            SortKey::NameDesc => list.sort_by(|a, b| b.name.cmp(&a.name)),
            SortKey::DateAsc => list.sort_by_key(|p| p.modified),
            SortKey::DateDesc => list.sort_by_key(|p| std::cmp::Reverse(p.modified)),
        }
    }
}

pub struct LauncherPanel {
    name: String,
    fps: u32,
    width: u32,
    height: u32,
    sample_rate: u32,
    channels: u32,
    status: String,
    search: String,
    sort: SortKey,
    selection_mode: bool,
    selected: HashSet<PathBuf>,
    pending_open: Option<PathBuf>,
}

impl LauncherPanel {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            fps: 30,
            width: 1920,
            height: 1080,
            sample_rate: 48000,
            channels: 2,
            status: String::new(),
            search: String::new(),
            sort: SortKey::NameAsc,
            selection_mode: false,
            selected: HashSet::new(),
            pending_open: None,
        }
    }

    fn new_project_card(&mut self, ui: &mut egui::Ui) -> Option<ProjectMeta> {
        let mut result = None;
        Card::new().heading(t!("新規プロジェクト")).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let muted = Theme::current(ui.ctx()).palette.text_muted;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.colored_label(muted, t!("映像"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.fps)
                                .range(1..=240)
                                .suffix(" fps"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.width)
                                .range(16..=7680)
                                .suffix(" px"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.height)
                                .range(16..=7680)
                                .suffix(" px"),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.vertical(|ui| {
                        ui.colored_label(muted, t!("音声"));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.channels)
                                    .range(1..=8)
                                    .suffix(" ch"),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.sample_rate)
                                    .range(8000..=192000)
                                    .suffix(" Hz"),
                            );
                        });
                    });
                });
            });
            ui.add_space(8.0);
            ui.colored_label(muted, t!("名前"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(Button::new(t!("作成して開く"))).clicked() {
                        match project::create_project(
                            &self.name,
                            self.fps,
                            self.width,
                            self.height,
                            self.sample_rate,
                            self.channels,
                        ) {
                            Ok(meta) => {
                                self.status.clear();
                                result = Some(meta);
                            }
                            Err(err) => self.status = err.to_string(),
                        }
                    }
                    ui.add(TextInput::new(&mut self.name).desired_width(ui.available_width()));
                });
            });
            if !self.status.is_empty() {
                ui.add_space(6.0);
                ui.colored_label(Theme::current(ui.ctx()).palette.danger, &self.status);
            }
        });
        result
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add(Button::new(self.sort.label()).outline()).clicked() {
                self.sort = self.sort.next();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let has_selection = !self.selected.is_empty();
                if ui
                    .add(
                        Button::new(t!("削除"))
                            .accent(Accent::Red)
                            .outline()
                            .enabled(has_selection),
                    )
                    .clicked()
                {
                    for dir in self.selected.drain() {
                        let _ = project::delete_project(&dir);
                    }
                }
                if ui
                    .add(Button::new(t!("コピー")).outline().enabled(has_selection))
                    .clicked()
                {
                    for dir in self.selected.iter() {
                        let _ = project::copy_project(dir);
                    }
                    self.selected.clear();
                }
                let toggle_label = if self.selection_mode {
                    t!("選択モードを出る")
                } else {
                    t!("選択モードに入る")
                };
                if ui.add(Button::new(toggle_label).outline()).clicked() {
                    self.selection_mode = !self.selection_mode;
                    if !self.selection_mode {
                        self.selected.clear();
                    }
                }
            });
        });
    }

    fn project_row(&mut self, ui: &mut egui::Ui, item: &ProjectMeta) {
        let theme = Theme::current(ui.ctx());
        let p = theme.palette.clone();
        ui.horizontal(|ui| {
            if self.selection_mode {
                let mut checked = self.selected.contains(&item.dir);
                if ui.add(Checkbox::new(&mut checked, "")).changed() {
                    if checked {
                        self.selected.insert(item.dir.clone());
                    } else {
                        self.selected.remove(&item.dir);
                    }
                }
            }
            let is_selected = self.selected.contains(&item.dir);
            let size = egui::vec2(ui.available_width(), 40.0);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            if ui.is_rect_visible(rect) {
                let hovered = response.hovered();
                let border = if is_selected {
                    p.text
                } else if hovered {
                    p.text_muted
                } else {
                    p.border
                };
                let fill = if is_selected {
                    egui::Color32::from_rgba_unmultiplied(p.text.r(), p.text.g(), p.text.b(), 20)
                } else if hovered {
                    egui::Color32::from_rgba_unmultiplied(
                        p.text_muted.r(),
                        p.text_muted.g(),
                        p.text_muted.b(),
                        15,
                    )
                } else {
                    egui::Color32::TRANSPARENT
                };
                ui.painter().rect(
                    rect,
                    egui::CornerRadius::same(theme.control_radius as u8),
                    fill,
                    egui::Stroke::new(1.0, border),
                    egui::StrokeKind::Inside,
                );
                let pad = 12.0;
                ui.painter().text(
                    egui::pos2(rect.left() + pad, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &item.name,
                    egui::FontId::proportional(theme.typography.body),
                    p.text,
                );
                let meta = format!(
                    "{} × {}  {}fps  ・  {}",
                    item.width,
                    item.height,
                    item.fps,
                    project::format_date(item.modified)
                );
                ui.painter().text(
                    egui::pos2(rect.right() - pad, rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    meta,
                    egui::FontId::proportional(theme.typography.small),
                    p.text_muted,
                );
            }
            if response.clicked() {
                if self.selection_mode {
                    if is_selected {
                        self.selected.remove(&item.dir);
                    } else {
                        self.selected.insert(item.dir.clone());
                    }
                } else {
                    self.pending_open = Some(item.dir.clone());
                }
            }
        });
        ui.add_space(6.0);
    }

    fn existing_projects_card(&mut self, ui: &mut egui::Ui) {
        Card::new().heading(t!("既存プロジェクト")).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let search_hint = t!("検索");
            ui.add(TextInput::new(&mut self.search).hint(search_hint.as_str()));
            ui.add_space(8.0);
            self.toolbar(ui);
            ui.add_space(8.0);

            let mut projects = project::list_projects();
            self.sort.apply(&mut projects);
            let query = self.search.to_lowercase();
            let projects: Vec<ProjectMeta> = projects
                .into_iter()
                .filter(|p| query.is_empty() || p.name.to_lowercase().contains(&query))
                .collect();

            if projects.is_empty() {
                ui.colored_label(
                    Theme::current(ui.ctx()).palette.text_faint,
                    t!("プロジェクトがありません"),
                );
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for item in &projects {
                        self.project_row(ui, item);
                    }
                });
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<ProjectMeta> {
        let mut result = None;
        let margin = egui::Margin::same(Theme::current(ui.ctx()).card_padding as i8);
        egui::Frame::new().inner_margin(margin).show(ui, |ui| {
            if let Some(meta) = self.new_project_card(ui) {
                result = Some(meta);
            }
            ui.add_space(16.0);
            self.existing_projects_card(ui);
        });

        if let Some(dir) = self.pending_open.take() {
            if let Some(meta) = project::load_project(&dir) {
                result = Some(meta);
            }
        }
        result
    }
}
