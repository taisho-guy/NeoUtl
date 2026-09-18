use crate::localization::tr;
use crate::ui::types::{ContextMenuItem, ObjectKindItem};

pub const CLIP_PALETTE_HEX: &[&str] = &[
    "#2a4db8", "#256e3c", "#6a2db8", "#b8862a", "#2ab8a8", "#b82a5f",
];

pub fn clip_color_hex(kind: i32, kind_known: bool, selected: bool) -> &'static str {
    if !kind_known {
        if selected { "#808080" } else { "#5a5a5a" }
    } else {
        let idx = (kind.max(0) as usize) % CLIP_PALETTE_HEX.len();
        CLIP_PALETTE_HEX[idx]
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

pub fn build_context_menu(
    hit_id: i32,
    clipboard_empty: bool,
    kinds: &[ObjectKindItem],
    _objects: &[(i32, String)],
    show_grid: bool,
    show_waveform: bool,
    select_range: Option<(i32, i32)>,
) -> Vec<ContextMenuItem> {
    let has_range = select_range.is_some();
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
            ContextMenuItem {
                label: "左側に詰める".into(),
                action: 18,
                kind: -1,
                enabled: true,
                icon: String::new(),
                checked: None,
                submenu: Vec::new(),
            },
            ContextMenuItem {
                label: "切り取りして詰める".into(),
                action: 19,
                kind: -1,
                enabled: true,
                icon: String::new(),
                checked: None,
                submenu: Vec::new(),
            },
            ContextMenuItem {
                label: "切り出し".into(),
                action: 20,
                kind: -1,
                enabled: has_range,
                icon: String::new(),
                checked: None,
                submenu: Vec::new(),
            },
        ];
    }

    let media_submenu: Vec<ContextMenuItem> = kinds
        .iter()
        .map(|k| ContextMenuItem {
            label: format!("{}を追加", &k.name),
            action: 2,
            kind: k.kind,
            enabled: true,
            icon: "circle-plus".into(),
            checked: None,
            submenu: Vec::new(),
        })
        .collect();

    vec![
        ContextMenuItem {
            label: "メディアオブジェクトを追加".into(),
            action: 17,
            kind: -1,
            enabled: !media_submenu.is_empty(),
            icon: "circle-plus".into(),
            checked: None,
            submenu: media_submenu,
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
        sep(),
        ContextMenuItem {
            label: "グリッド(BPM)の表示".into(),
            action: 15,
            kind: -1,
            enabled: true,
            icon: "grid".into(),
            checked: Some(show_grid),
            submenu: Vec::new(),
        },
        ContextMenuItem {
            label: "音声波形の表示".into(),
            action: 16,
            kind: -1,
            enabled: true,
            icon: "audio-lines".into(),
            checked: Some(show_waveform),
            submenu: Vec::new(),
        },
    ]
}

pub fn build_layer_menu(
    layer: i32,
    layer_states: &[(bool, bool)],
    show_grid: bool,
    show_waveform: bool,
) -> Vec<ContextMenuItem> {
    let (visible, locked) = layer_states
        .get(layer as usize)
        .copied()
        .unwrap_or((true, false));

    vec![
        ContextMenuItem {
            label: "レイヤーのロック".into(),
            action: 40,
            kind: layer,
            enabled: true,
            icon: "lock".into(),
            checked: Some(locked),
            submenu: Vec::new(),
        },
        ContextMenuItem {
            label: "レイヤーの表示".into(),
            action: 41,
            kind: layer,
            enabled: true,
            icon: "eye".into(),
            checked: Some(visible),
            submenu: Vec::new(),
        },
        sep(),
        ContextMenuItem {
            label: "グリッド(BPM)の表示".into(),
            action: 15,
            kind: -1,
            enabled: true,
            icon: "grid".into(),
            checked: Some(show_grid),
            submenu: Vec::new(),
        },
        ContextMenuItem {
            label: "音声波形の表示".into(),
            action: 16,
            kind: -1,
            enabled: true,
            icon: "audio-lines".into(),
            checked: Some(show_waveform),
            submenu: Vec::new(),
        },
    ]
}
