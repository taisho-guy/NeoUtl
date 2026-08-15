fn main() {
    neoutl_object_shader_build::compile_compute_spirv(
        "semi_planar_to_rgba",
        "slang/semi_planar_to_rgba.slang",
        "cs_main",
    );

    let avutil = pkg_config::probe_library("libavutil").expect("pkg-config libavutil取得失敗");
    let libva = pkg_config::probe_library("libva").expect("pkg-config libva取得失敗");
    let mut build = cc::Build::new();
    build.file("csrc/vk_device_ctx_shim.c");
    for path in &avutil.include_paths {
        build.include(path);
    }
    for path in &libva.include_paths {
        build.include(path);
    }
    build.compile("neoutl_vk_device_ctx_shim");
    println!("cargo:rustc-link-lib=va");

    println!("cargo:rerun-if-changed=csrc/vk_device_ctx_shim.c");
}
