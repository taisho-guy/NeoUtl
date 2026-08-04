//! アプリ全体のテーマを`egui-elegance`の`BuiltInTheme`(Slate/Charcoal/Frost/Paper)一本化で管理する。
//!
//! 旧`neoutl-theme-api`ベースのプラグイン方式(壁紙連動色解決・`theme::registry()`/`resolve()`等)は廃止した。
//! 本アプリは`egui_loop.rs`で複数の独立ネイティブウィンドウ(=複数の`egui::Context`)を持つため、
//! elegance公式の「1コンテキストにつき`Theme::install`を毎フレーム呼ぶ」想定をそのまま使うと
//! ウィンドウ間でテーマが揃わない。そこでプロセス全体で共有する現在選択値をここに保持し、
//! 各ウィンドウのredrawループから`theme::install(ctx)`を毎フレーム呼んで同期させる
//! (`install`自体は変更が無ければ低コストとelegance側ドキュメントに明記されている)。

use elegance::BuiltInTheme;
use std::sync::Mutex;

static CURRENT: Mutex<BuiltInTheme> = Mutex::new(BuiltInTheme::Slate);

/// 現在選択中のテーマ。
pub fn current() -> BuiltInTheme {
    *CURRENT.lock().unwrap()
}

/// 選択テーマを変更する。次フレームから全ウィンドウへ反映される。
pub fn set(theme: BuiltInTheme) {
    *CURRENT.lock().unwrap() = theme;
}

/// `SystemSettingsResource::theme_id`永続化用の安定文字列ID。
pub fn id_of(theme: BuiltInTheme) -> &'static str {
    match theme {
        BuiltInTheme::Slate => "slate",
        BuiltInTheme::Charcoal => "charcoal",
        BuiltInTheme::Frost => "frost",
        BuiltInTheme::Paper => "paper",
        _ => "slate",
    }
}

/// 安定文字列IDから復元する。未知の値は既定(Slate)にフォールバックする。
pub fn from_id(id: &str) -> BuiltInTheme {
    match id {
        "charcoal" => BuiltInTheme::Charcoal,
        "frost" => BuiltInTheme::Frost,
        "paper" => BuiltInTheme::Paper,
        _ => BuiltInTheme::Slate,
    }
}

/// 起動時、`SystemSettingsResource`から読み込んだ保存済みIDを反映する。
pub fn restore(theme_id: &str) {
    set(from_id(theme_id));
}

/// 各ネイティブウィンドウのredrawループから毎フレーム呼ぶ。
pub fn install(ctx: &egui::Context) {
    current().theme().install(ctx);
}
