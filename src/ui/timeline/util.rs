use crate::localization::tr;
use crate::ui::types::{ContextMenuItem, ObjectKindItem};
use egui::Color32;

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

pub(super) fn readable_text_color(bg: Color32) -> Color32 {
    let channel = |v: u8| {
        let c = v as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(bg.r()) + 0.7152 * channel(bg.g()) + 0.0722 * channel(bg.b());
    if luminance > 0.5 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn sep() -> ContextMenuItem {
    ContextMenuItem {
        label: String::new(),
        action: 4,
        kind: -1,
        enabled: false,
        icon: String::new(),
        checked: None,
        submenu: Vec::new(),
    }
}

fn disabled_leaf(label: String, action: i32) -> ContextMenuItem {
    ContextMenuItem {
        label,
        action,
        kind: -1,
        enabled: false,
        icon: String::new(),
        checked: None,
        submenu: Vec::new(),
    }
}

fn disabled_submenu_parent(label: String) -> ContextMenuItem {
    ContextMenuItem {
        label,
        action: 17,
        kind: -1,
        enabled: false,
        icon: String::new(),
        checked: None,
        submenu: Vec::new(),
    }
}

pub(super) fn build_context_menu(
    hit_id: i32,
    clipboard_empty: bool,
    kinds: &[ObjectKindItem],
    objects: &[(i32, String)],
    show_grid: bool,
    show_waveform: bool,
) -> Vec<ContextMenuItem> {
    if hit_id >= 0 {
        return vec![
            ContextMenuItem {
                label: tr("切り取り"),
                action: 8,
                kind: -1,
                enabled: true,
                icon: "scissors".into(),
                checked: None,
                submenu: Vec::new(),
            },
            ContextMenuItem {
                label: tr("コピー"),
                action: 9,
                kind: -1,
                enabled: true,
                icon: "copy".into(),
                checked: None,
                submenu: Vec::new(),
            },
            ContextMenuItem {
                label: tr("貼り付け"),
                action: 10,
                kind: -1,
                enabled: !clipboard_empty,
                icon: "paste".into(),
                checked: None,
                submenu: Vec::new(),
            },
            ContextMenuItem {
                label: tr("削除"),
                action: 1,
                kind: -1,
                enabled: true,
                icon: "trash".into(),
                checked: None,
                submenu: Vec::new(),
            },
            ContextMenuItem {
                label: tr("複製"),
                action: 7,
                kind: -1,
                enabled: true,
                icon: "copy-plus".into(),
                checked: None,
                submenu: Vec::new(),
            },
            ContextMenuItem {
                label: tr("分割"),
                action: 0,
                kind: -1,
                enabled: true,
                icon: "scissors".into(),
                checked: None,
                submenu: Vec::new(),
            },
            sep(),
            disabled_leaf(t!("左側に詰める"), 18),
            disabled_leaf(t!("切り取りして詰める"), 19),
            disabled_leaf(t!("切り出し"), 20),
            disabled_leaf(t!("長さを変更"), 21),
            sep(),
            disabled_submenu_parent(t!("整列")),
            sep(),
            disabled_leaf(t!("オブジェクト名を変更"), 22),
            sep(),
            disabled_leaf(t!("中間点を追加"), 23),
            disabled_leaf(t!("中間点を削除"), 24),
            sep(),
            disabled_leaf(t!("グループ化"), 25),
            disabled_leaf(t!("グループ解除"), 26),
            sep(),
            disabled_leaf(t!("エイリアスをファイルに保存"), 27),
            disabled_leaf(t!("エイリアスを作成"), 28),
        ];
    }

    let media_submenu: Vec<ContextMenuItem> = kinds
        .iter()
        .map(|k| ContextMenuItem {
            label: t!("{}を追加").replace("{}", &k.name),
            action: 2,
            kind: k.kind,
            enabled: true,
            icon: "circle-plus".into(),
            checked: None,
            submenu: Vec::new(),
        })
        .collect();

    let object_select_submenu: Vec<ContextMenuItem> = objects
        .iter()
        .map(|(id, label)| ContextMenuItem {
            label: label.clone(),
            action: 14,
            kind: *id,
            enabled: true,
            icon: "mouse-pointer-click".into(),
            checked: None,
            submenu: Vec::new(),
        })
        .collect();
    let object_select_enabled = !object_select_submenu.is_empty();

    vec![
        ContextMenuItem {
            label: t!("メディアオブジェクトを追加"),
            action: 17,
            kind: -1,
            enabled: !media_submenu.is_empty(),
            icon: "circle-plus".into(),
            checked: None,
            submenu: media_submenu,
        },
        disabled_submenu_parent(t!("フィルタオブジェクトを追加")),
        sep(),
        disabled_submenu_parent(t!("フィルタ効果を追加")),
        sep(),
        ContextMenuItem {
            label: t!("貼り付け"),
            action: 10,
            kind: -1,
            enabled: !clipboard_empty,
            icon: "paste".into(),
            checked: None,
            submenu: Vec::new(),
        },
        disabled_leaf(t!("空のフレームを挿入"), 11),
        sep(),
        disabled_leaf(t!("選択範囲を切り取り"), 12),
        disabled_leaf(t!("選択範囲を切り取りして詰める"), 13),
        sep(),
        ContextMenuItem {
            label: t!("オブジェクト選択"),
            action: 17,
            kind: -1,
            enabled: object_select_enabled,
            icon: "list".into(),
            checked: None,
            submenu: object_select_submenu,
        },
        disabled_submenu_parent(t!("プラグイン")),
        sep(),
        ContextMenuItem {
            label: t!("グリッド(BPM)の表示"),
            action: 15,
            kind: -1,
            enabled: true,
            icon: "grid".into(),
            checked: Some(show_grid),
            submenu: Vec::new(),
        },
        ContextMenuItem {
            label: t!("音声波形の表示"),
            action: 16,
            kind: -1,
            enabled: true,
            icon: "audio-lines".into(),
            checked: Some(show_waveform),
            submenu: Vec::new(),
        },
        sep(),
        disabled_submenu_parent(t!("オプション")),
        disabled_submenu_parent(t!("ウィンドウ配置")),
    ]
}

pub(super) fn build_layer_menu(
    layer: i32,
    layer_states: &[(bool, bool)],
    show_grid: bool,
    show_waveform: bool,
) -> Vec<ContextMenuItem> {
    let (visible, locked) = layer_states
        .get(layer as usize)
        .copied()
        .unwrap_or((true, false));

    let visibility_submenu: Vec<ContextMenuItem> = layer_states
        .iter()
        .enumerate()
        .map(|(idx, &(vis, _))| ContextMenuItem {
            label: t!("レイヤー{}").replace("{}", &(idx + 1).to_string()),
            action: 41,
            kind: idx as i32,
            enabled: true,
            icon: "eye".into(),
            checked: Some(vis),
            submenu: Vec::new(),
        })
        .collect();

    vec![
        ContextMenuItem {
            label: t!("レイヤーのロック"),
            action: 40,
            kind: layer,
            enabled: true,
            icon: "lock".into(),
            checked: Some(locked),
            submenu: Vec::new(),
        },
        ContextMenuItem {
            label: t!("レイヤーの表示"),
            action: 41,
            kind: layer,
            enabled: true,
            icon: "eye".into(),
            checked: Some(visible),
            submenu: Vec::new(),
        },
        disabled_leaf(t!("レイヤーを設定"), 42),
        disabled_leaf(t!("レイヤー名を変更"), 43),
        disabled_leaf(t!("他のレイヤーを表示/非表示"), 44),
        sep(),
        disabled_leaf(t!("レイヤーを挿入"), 45),
        disabled_leaf(t!("レイヤーを削除"), 46),
        sep(),
        ContextMenuItem {
            label: t!("レイヤーの表示"),
            action: 17,
            kind: -1,
            enabled: !visibility_submenu.is_empty(),
            icon: "list".into(),
            checked: None,
            submenu: visibility_submenu,
        },
        sep(),
        ContextMenuItem {
            label: t!("グリッド(BPM)の表示"),
            action: 15,
            kind: -1,
            enabled: true,
            icon: "grid".into(),
            checked: Some(show_grid),
            submenu: Vec::new(),
        },
        ContextMenuItem {
            label: t!("音声波形の表示"),
            action: 16,
            kind: -1,
            enabled: true,
            icon: "audio-lines".into(),
            checked: Some(show_waveform),
            submenu: Vec::new(),
        },
        sep(),
        disabled_submenu_parent(t!("オプション")),
        disabled_submenu_parent(t!("ウィンドウ配置")),
    ]
}

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
