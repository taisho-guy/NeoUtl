//! GlitchTip(Sentry互換)へのクラッシュ/エラー匿名送信。既定無効（オプトイン、
//! システム設定`crash_reporting_enabled`経由のみ有効化）。有効化はプロセス起動時
//! 1回限りで、実行中の動的切替は次回起動まで反映しない。

use crate::config;

/// `enabled == false`の場合SDK初期化自体を行わず、通信は一切発生しない。
/// 戻り値`ClientInitGuard`は呼出元(main関数)のスコープで保持し、早期dropしない。
/// panic機能を有効化しているため、release/debug双方でパニック発生時に自動送信される
/// (`[profile.release] panic = "unwind"`前提、abortでは送信猶予がなく機能しない)。
pub fn init(enabled: bool) -> Option<sentry::ClientInitGuard> {
    if !enabled {
        return None;
    }
    let options = sentry::ClientOptions::new()
        .dsn(config::SENTRY_DSN)
        .maybe_release(sentry::release_name!())
        .traces_sample_rate(0.01);
    Some(sentry::init(options))
}

/// 未処理には至らないが記録すべきエラーの手動送信。
pub fn capture_error(err: &dyn std::error::Error) {
    sentry::capture_error(err);
}

/// 任意メッセージの手動送信（診断用）。
pub fn capture_message(msg: &str) {
    sentry::capture_message(msg, sentry::Level::Error);
}
