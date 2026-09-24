# NeoUtl Plugin SDK (Rust) - v0.7.5

NeoUtl 向けのオールマイティなプラグイン開発用公式 Rust SDK です。
本 SDK を利用することで、以下のすべての種類の NeoUtl プラグインを高速かつ型安全に作成できます：

1. **拡張プラグイン (Extensions / 超拡張・魔改造)**:
   - egui ネイティブ UI パネル（浮動ウィンドウ／ドック可能）
   - プレビューおよびタイムラインキャンバスへの直接オーバーレイ描画
   - メニューバー（ファイル・編集・ツール・プラグイン）およびコンテキストメニューへの項目注入
   - ショートカットキー付きコマンド登録
   - トランザクション対応タイムライン自動編集（`EditSession`：自動 Undo/Redo）
   - プロジェクト保存連動のキーバリューストレージ（`ProjectStorage`）
   - 任意ファイルのドラッグ＆ドロップインターセプト（字幕、CSV、外部アセット等）
2. **エフェクトプラグイン (Effects)**:
   - GPU ハードウェアアクセラレーション（WGSL シェーダー）
   - パラメータスキーマ定義（スライダー、カラーピッカー、チェックボックス、ドロップダウン）
   - リアルタイム ROI（描画領域）計算、オーディオ DSP 処理
3. **オブジェクトプラグイン (Objects)**:
   - タイムライン上に配置可能な 2D / 3D 幾何形状、テキスト、メディアオブジェクト
   - 頂点バッファとシェーダーのカスタム描画
4. **イージングプラグイン (Easing Curves)**:
   - キーフレーム間の補間計算エンジン、カスタムグラフエディタ連携
5. **数式エンジン (Expressions)**:
   - プロパティ式評価、変数バインディング、数式構文解析

---

## 1. プロジェクトの始め方 (Cargo.toml)

プラグインは動的共有ライブラリ（Linux では `.so`、Windows では `.dll`、macOS では `.dylib`）としてビルドします。

```toml
[package]
name = "my_neoutl_plugin"
version = "1.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
neoutl-sdk = { path = "path/to/sdk/neoutl/0.7.5/rust" }
# もしくは crates.io 登録後は:
# neoutl-sdk = "0.7.5"
```

---

## 2. 実装例

### (A) 拡張プラグイン（GUI・自動化・魔改造）

```rust
use neoutl_sdk::prelude::*;

#[derive(Default)]
pub struct SuperModPlugin;

impl ExtensionPlugin for SuperModPlugin {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata::new("com.example.super_mod", "スーパー拡張プラグイン")
    }

    fn init(&mut self, ctx: &mut ExtensionContext) -> Result<(), String> {
        // パネル登録
        ctx.register_panel(PanelDescriptor {
            id: "super_panel".to_owned(),
            title: "魔改造コントロール".to_owned(),
            default_size: [320.0, 240.0],
            default_open: true,
            placement: PanelPlacement::FloatingWindow,
        });

        // ツールメニューに登録
        ctx.register_menu(MenuItemDescriptor {
            id: "menu_align_clips".to_owned(),
            location: MenuLocation::MenuBar(MenuBarSection::Tools),
            label: "全クリップを整列".to_owned(),
            shortcut_hint: Some("Ctrl+Shift+A".to_owned()),
            command_id: "super_mod.align".to_owned(),
            enabled: true,
            checked: None,
        });

        Ok(())
    }

    fn ui(&mut self, panel_id: &str, ui: &mut egui::Ui, ctx: &mut ExtensionContext) {
        if panel_id == "super_panel" {
            ui.heading("⚡ 超拡張パネル");
            if ui.button("新規テキストクリップ作成").clicked() {
                if let Some(session) = &mut ctx.edit_session {
                    let cur = session.cursor_frame();
                    let _ = session.create_text_object("Hello NeoUtl!", 1, cur, 90);
                    ctx.show_toast("テキストを追加しました");
                }
            }
        }
    }
}

// エントリポイントのエクスポート
neoutl_sdk::export_extension!(SuperModPlugin);
```

### (B) エフェクトプラグイン（WGSL シェーダー）

```rust
use neoutl_sdk::prelude::*;
use std::sync::OnceLock;

static PARAM_SCHEMA: &[EffectParamSchema] = &[
    EffectParamSchema {
        key: str_ref("intensity"),
        label: str_ref("強度"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 1.0,
        step: 0.01,
        default_float: 0.5,
        enum_options: empty_str_ref(),
    },
];

static META: EffectMeta = EffectMeta {
    id: "custom_tint",
    name: "Custom Tint Filter",
    category: "Color",
    param_schema: ffi_slice(PARAM_SCHEMA),
    kind: EffectKind::Image,
    author: str_ref("NeoUtl Dev"),
    description: empty_str_ref(),
    uuid: str_ref("custom_tint"),
    is_dummy: 0,
    use_composition_camera: 0,
};

static WGSL_CODE: &[u8] = include_bytes!("shader.wgsl");
static VTABLE: OnceLock<EffectVTable> = OnceLock::new();

unsafe extern "C" fn meta() -> *const EffectMeta { &raw const META }
unsafe extern "C" fn wgsl() -> WgslSource {
    WgslSource { ptr: WGSL_CODE.as_ptr(), len: WGSL_CODE.len() }
}
unsafe extern "C" fn uniform_size() -> u32 { uniform_size_std(PARAM_SCHEMA.len() as u32) }
unsafe extern "C" fn pack_uniform(params_ptr: *const f32, count: u32, out_ptr: *mut u8) {
    unsafe { pack_uniform_std(params_ptr, count, out_ptr) }
}

pub fn get_vtable() -> *const EffectVTable {
    VTABLE.get_or_init(|| EffectVTable {
        meta,
        wgsl,
        uniform_size,
        pack_uniform,
        requires_texture_param: None,
        calc_roi: None,
        is_need_render_frame: None,
        process_audio: None,
        on_property_edited: None,
        on_property_restored: None,
        poll_writeback: None,
        setup_accelerator: None,
    })
}

neoutl_sdk::export_effect!(get_vtable);
```

---

## 3. ビルドと配置

```bash
# ビルド
cargo build --release

# NeoUtl本体の対応ディレクトリへコピー
# 拡張プラグインの場合:
cp target/release/libmy_neoutl_plugin.so path/to/NeoUtl/extensions/

# エフェクトプラグインの場合:
cp target/release/libmy_neoutl_plugin.so path/to/NeoUtl/effects/

# オブジェクトプラグインの場合:
cp target/release/libmy_neoutl_plugin.so path/to/NeoUtl/objects/
```
