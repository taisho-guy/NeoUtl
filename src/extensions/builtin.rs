use neoutl_extension_api::*;

pub struct QuickPaletteExtension {
    meta: ExtensionMetadata,
    custom_script_input: String,
    last_exec_status: String,
}

impl QuickPaletteExtension {
    pub fn new() -> Self {
        let mut meta = ExtensionMetadata::new(
            "neoutl.builtin.quick_palette",
            "クイックパレット & タイムライン自動化",
        );
        meta.author = "NeoUtl Core Team".to_owned();
        meta.description =
            "タイムラインのバッチ処理やマクロ自動化、HUD表示を提供する標準拡張プラグイン"
                .to_owned();
        meta.version = "1.0.0".to_owned();
        Self {
            meta,
            custom_script_input: "edit:create_text_object('Hello NeoUtl', 1, 0, 150)".to_owned(),
            last_exec_status: "待機中".to_owned(),
        }
    }
}

impl ExtensionPlugin for QuickPaletteExtension {
    fn metadata(&self) -> ExtensionMetadata {
        self.meta.clone()
    }

    fn init(&mut self, ctx: &mut ExtensionContext) -> Result<(), String> {
        ctx.register_panel(PanelDescriptor {
            id: "quick_palette".to_owned(),
            title: "クイックパレット & 自動化".to_owned(),
            default_size: [360.0, 320.0],
            default_open: false,
            placement: PanelPlacement::FloatingWindow,
        });

        ctx.register_menu(MenuItemDescriptor {
            id: "menu_quick_palette".to_owned(),
            location: MenuLocation::MenuBar(MenuBarSection::Plugins),
            label: "クイックパレットを表示".to_owned(),
            shortcut_hint: Some("Ctrl+Shift+P".to_owned()),
            command_id: "quick_palette.toggle".to_owned(),
            enabled: true,
            checked: None,
        });

        ctx.register_menu(MenuItemDescriptor {
            id: "menu_batch_align".to_owned(),
            location: MenuLocation::MenuBar(MenuBarSection::Tools),
            label: "選択オブジェクトの整列 (レイヤー詰め)".to_owned(),
            shortcut_hint: None,
            command_id: "quick_palette.align_selected".to_owned(),
            enabled: true,
            checked: None,
        });

        ctx.register_command(CommandDescriptor {
            id: "quick_palette.toggle".to_owned(),
            name: "クイックパレットの表示切替".to_owned(),
            description: "クイックパレットウィンドウを開閉します".to_owned(),
            default_shortcut: Some("Ctrl+Shift+P".to_owned()),
        });

        ctx.register_command(CommandDescriptor {
            id: "quick_palette.align_selected".to_owned(),
            name: "選択オブジェクトの整列".to_owned(),
            description: "選択中のオブジェクトを前のオブジェクトの直後に整列します".to_owned(),
            default_shortcut: None,
        });

        Ok(())
    }

    fn ui(&mut self, panel_id: &str, ui: &mut egui::Ui, ctx: &mut ExtensionContext) {
        if panel_id != "quick_palette" {
            return;
        }

        ui.heading("⚡ タイムライン自動化パレット");
        ui.separator();

        ui.add_space(8.0);
        ui.label(egui::RichText::new("クイック自動編集アクション").strong());

        let has_project = ctx.edit_session.is_some();
        ui.add_enabled_ui(has_project, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("➕ カーソル位置に字幕テキスト追加").clicked() {
                    if let Some(session) = &mut ctx.edit_session {
                        let cur = session.cursor_frame();
                        match session.create_text_object("新規テキスト", 1, cur, 90) {
                            Ok(id) => {
                                ctx.show_toast(format!(
                                    "テキストオブジェクト (ID: {id}) を追加しました"
                                ));
                            }
                            Err(e) => ctx.show_toast(format!("追加失敗: {e}")),
                        }
                    }
                }

                if ui
                    .button("⏹️ 選択クリップの不透明度を100%にリセット")
                    .clicked()
                {
                    if let Some(session) = &mut ctx.edit_session {
                        let selected = session.selected_object_ids();
                        for id in selected {
                            let _ = session.set_param_float(id, None, "opacity", 1.0);
                        }
                        ctx.show_toast("不透明度をリセットしました");
                    }
                }
            });
        });

        if !has_project {
            ui.colored_label(
                egui::Color32::YELLOW,
                "※プロジェクトを開くと編集機能が有効になります",
            );
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label(egui::RichText::new("拡張ストレージ (KVストア)").strong());
        let current_memo = ctx.storage.get_string("user_memo").unwrap_or("").to_owned();
        let mut memo_buf = current_memo.clone();
        ui.horizontal(|ui| {
            ui.label("プロジェクト内メモ:");
            if ui.text_edit_singleline(&mut memo_buf).changed() {
                ctx.storage.set_string("user_memo", memo_buf);
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("クイックテキスト:");
            ui.text_edit_singleline(&mut self.custom_script_input);
            if ui.button("挿入").clicked() {
                if let Some(session) = &mut ctx.edit_session {
                    let cur = session.cursor_frame();
                    match session.create_text_object(&self.custom_script_input, 1, cur, 90) {
                        Ok(id) => {
                            self.last_exec_status = format!("テキスト追加成功 (ID: {id})");
                            ctx.show_toast(format!("テキスト (ID: {id}) を追加しました"));
                        }
                        Err(e) => {
                            self.last_exec_status = format!("エラー: {e}");
                            ctx.show_toast(format!("追加失敗: {e}"));
                        }
                    }
                }
            }
        });

        ui.add_space(8.0);
        ui.label(format!("ステータス: {}", self.last_exec_status));
    }

    fn on_command(&mut self, command_id: &str, ctx: &mut ExtensionContext) -> Result<(), String> {
        match command_id {
            "quick_palette.toggle" => {
                ctx.show_toast("クイックパレットを切り替えました");
                Ok(())
            }
            "quick_palette.align_selected" => {
                if let Some(session) = &mut ctx.edit_session {
                    let selected = session.selected_object_ids();
                    if selected.is_empty() {
                        return Err("整列対象のオブジェクトが選択されていません".to_owned());
                    }
                    let mut objects: Vec<_> = selected
                        .iter()
                        .filter_map(|&id| session.get_object(id))
                        .collect();
                    objects.sort_by_key(|o| o.start_frame);

                    for i in 1..objects.len() {
                        let prev_end = objects[i - 1].end_frame;
                        let cur_id = objects[i].id;
                        let cur_layer = objects[i].layer;
                        let cur_dur = objects[i].duration;
                        let _ = session.move_object(cur_id, cur_layer, prev_end);
                        objects[i].start_frame = prev_end;
                        objects[i].end_frame = prev_end + cur_dur;
                    }
                    ctx.show_toast(format!("{} 個のオブジェクトを整列しました", objects.len()));
                    Ok(())
                } else {
                    Err("プロジェクトが開かれていません".to_owned())
                }
            }
            _ => Ok(()),
        }
    }
}

pub struct SubtitleImporterExtension {
    meta: ExtensionMetadata,
}

impl SubtitleImporterExtension {
    pub fn new() -> Self {
        let mut meta = ExtensionMetadata::new(
            "neoutl.builtin.subtitle_importer",
            "字幕 / SRT自動インポーター",
        );
        meta.author = "NeoUtl Core Team".to_owned();
        meta.description = "SRT/VTT字幕ファイルをドラッグ＆ドロップで自動配置する拡張".to_owned();
        Self { meta }
    }
}

impl ExtensionPlugin for SubtitleImporterExtension {
    fn metadata(&self) -> ExtensionMetadata {
        self.meta.clone()
    }

    fn init(&mut self, ctx: &mut ExtensionContext) -> Result<(), String> {
        ctx.register_file_drop(FileDropDescriptor {
            name: "字幕ファイルインポート (SRT/VTT)".to_owned(),
            file_extensions: vec!["srt".to_owned(), "vtt".to_owned()],
        });

        ctx.register_menu(MenuItemDescriptor {
            id: "menu_import_srt".to_owned(),
            location: MenuLocation::Import,
            label: "字幕ファイル (.srt / .vtt)...".to_owned(),
            shortcut_hint: None,
            command_id: "subtitle.import_dialog".to_owned(),
            enabled: true,
            checked: None,
        });

        Ok(())
    }

    fn on_file_drop(
        &mut self,
        path: &str,
        layer: i32,
        drop_frame: i32,
        ctx: &mut ExtensionContext,
    ) -> bool {
        let p = std::path::Path::new(path);
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "srt" && ext != "vtt" {
            return false;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                ctx.show_toast(format!("字幕ファイルの読み込み失敗: {e}"));
                return false;
            }
        };

        if let Some(session) = &mut ctx.edit_session {
            let fps = session.project_info().fps.max(1);
            let target_layer = if layer > 0 { layer } else { 2 };
            let mut added_count = 0;

            let mut cur_text = String::new();
            let mut start_sec = 0.0f64;
            let mut end_sec = 0.0f64;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.contains("-->") {
                    let parts: Vec<&str> = trimmed.split("-->").collect();
                    if parts.len() == 2 {
                        start_sec = parse_srt_time(parts[0].trim());
                        end_sec = parse_srt_time(parts[1].trim());
                    }
                } else if !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_digit()) {
                    if !cur_text.is_empty() {
                        cur_text.push('\n');
                    }
                    cur_text.push_str(trimmed);
                } else if trimmed.is_empty() && !cur_text.is_empty() {
                    let start_frame = drop_frame + (start_sec * fps as f64).round() as i32;
                    let duration = ((end_sec - start_sec) * fps as f64).round().max(1.0) as i32;
                    if session
                        .create_text_object(&cur_text, target_layer, start_frame, duration)
                        .is_ok()
                    {
                        added_count += 1;
                    }
                    cur_text.clear();
                }
            }

            if !cur_text.is_empty() {
                let start_frame = drop_frame + (start_sec * fps as f64).round() as i32;
                let duration = ((end_sec - start_sec) * fps as f64).round().max(1.0) as i32;
                if session
                    .create_text_object(&cur_text, target_layer, start_frame, duration)
                    .is_ok()
                {
                    added_count += 1;
                }
            }

            ctx.show_toast(format!(
                "字幕ファイルから {} 件のテキストを追加しました",
                added_count
            ));
            true
        } else {
            false
        }
    }
}

fn parse_srt_time(s: &str) -> f64 {
    let s = s.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0].parse().unwrap_or(0.0);
        let m: f64 = parts[1].parse().unwrap_or(0.0);
        let s: f64 = parts[2].parse().unwrap_or(0.0);
        h * 3600.0 + m * 60.0 + s
    } else {
        0.0
    }
}
