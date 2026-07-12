fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        pkg_config::probe_library("gstreamer-d3d11-1.0").expect("gstreamer-d3d11-1.0未検出");
    }
}
