use crate::localization::tr;
use crate::ui::types::{ContextMenuItem, ObjectKindItem};
use egui::Color32;

/// 汎用ヘルパー群。egui::Colorの明暗調整、右クリックメニュー項目の構築、
/// egui::KeyのショートカットDSL用文字列化。TimelineWindowの状態に依存しない。
pub(super) fn brighten(c: Color32, factor: f32) -> Color32 {
    let f = |v: u8| {
        (v as f32 + (255.0 - v as f32) * factor)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

pub(super) fn darken(c: Color32, factor: f32) -> Color32 {
    let f = |v: u8| (v as f32 * (1.0 - factor)).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// タイムライン右クリックメニューの項目集合を構築する唯一の経路。
/// hit-id>=0（クリップ上）: 削除→分割→複製→区切り→切り取り→コピー→区切り→リップルモード切替。
/// hit-id<0（背景上）: 登録済みオブジェクト種別ごとのAdd項目→区切り→元に戻す→やり直す→貼り付け。

pub(super) fn build_context_menu(
    hit_id: i32,
    ripple_mode: bool,
    clipboard_empty: bool,
    kinds: &[ObjectKindItem],
) -> Vec<ContextMenuItem> {
    let sep = || ContextMenuItem {
        label: String::new(),
        action: 4,
        kind: -1,
        enabled: false,
        icon: String::new(),
    };
    if hit_id >= 0 {
        return vec![
            ContextMenuItem {
                label: tr("削除"),
                action: 1,
                kind: -1,
                enabled: true,
                icon: "trash".into(),
            },
            ContextMenuItem {
                label: tr("再生位置で分割"),
                action: 0,
                kind: -1,
                enabled: true,
                icon: "scissors".into(),
            },
            ContextMenuItem {
                label: tr("複製"),
                action: 7,
                kind: -1,
                enabled: true,
                icon: "copy-plus".into(),
            },
            sep(),
            ContextMenuItem {
                label: tr("切り取り"),
                action: 8,
                kind: -1,
                enabled: true,
                icon: "scissors".into(),
            },
            ContextMenuItem {
                label: tr("コピー"),
                action: 9,
                kind: -1,
                enabled: true,
                icon: "copy".into(),
            },
            sep(),
            ContextMenuItem {
                label: if ripple_mode {
                    tr("リップルモード: オン")
                } else {
                    tr("リップルモード: オフ")
                },
                action: 3,
                kind: -1,
                enabled: true,
                icon: "link".into(),
            },
        ];
    }
    let mut items: Vec<ContextMenuItem> = kinds
        .iter()
        .map(|k| ContextMenuItem {
            label: t!("{}を追加").replace("{}", &k.name),
            action: 2,
            kind: k.kind,
            enabled: true,
            icon: "circle-plus".into(),
        })
        .collect();
    items.push(sep());
    items.push(ContextMenuItem {
        label: t!("元に戻す"),
        action: 5,
        kind: -1,
        enabled: true,
        icon: "undo".into(),
    });
    items.push(ContextMenuItem {
        label: t!("やり直す"),
        action: 6,
        kind: -1,
        enabled: true,
        icon: "redo".into(),
    });
    items.push(ContextMenuItem {
        label: t!("貼り付け"),
        action: 10,
        kind: -1,
        enabled: !clipboard_empty,
        icon: "paste".into(),
    });

    items
}

/// ui::preview（プロジェクトタブショートカット解決）と共有するためcrate公開。
pub(crate) fn egui_key_name(key: egui::Key) -> String {
    use egui::Key;
    match key {
        Key::Space => "Space".into(),
        Key::ArrowRight => "Right".into(),
        Key::ArrowLeft => "Left".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::F2 => "F2".into(),
        Key::F3 => "F3".into(),
        Key::F4 => "F4".into(),
        Key::F9 => "F9".into(),
        Key::F10 => "F10".into(),
        Key::F11 => "F11".into(),
        Key::F12 => "F12".into(),
        Key::Tab => "Tab".into(),
        Key::PageDown => "PageDown".into(),
        Key::PageUp => "PageUp".into(),
        Key::Delete => "Delete".into(),
        Key::Equals => "=".into(),
        Key::Minus => "-".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}
