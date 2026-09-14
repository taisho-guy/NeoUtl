use crate::config;

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

pub fn capture_error(err: &dyn std::error::Error) {
    sentry::capture_error(err);
}

pub fn capture_message(msg: &str) {
    sentry::capture_message(msg, sentry::Level::Error);
}
