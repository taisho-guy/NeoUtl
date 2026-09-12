fn main() {
    let gui_private_include = std::process::Command::new("pkg-config")
        .args(["--variable=includedir", "Qt6Gui"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "/usr/include/qt6".to_string());

    let qt_version = std::process::Command::new("pkg-config")
        .args(["--modversion", "Qt6Gui"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let mut builder = cxx_qt_build::CxxQtBuilder::new()
        .qt_module("Quick")
        .qt_module("Qml")
        .qrc("qml/qml.qrc")
        .cpp_file("include/wgpu_rhi_item.h")
        .file("src/ffi.rs");

    builder = unsafe {
        builder.cc_builder(|cc| {
            cc.file("src/wgpu_rhi_item.cpp")
                .include("include")
                .include(format!("{gui_private_include}/QtGui/{qt_version}"))
                .include(format!("{gui_private_include}/QtGui/{qt_version}/QtGui"))
                .flag_if_supported("-std=c++17");
        })
    };

    builder.build();
}
