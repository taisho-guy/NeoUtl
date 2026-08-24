fn main() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("neoutl_descriptor.bin");

    let mut config = prost_build::Config::new();
    config.file_descriptor_set_path(&descriptor_path);
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config
        .compile_protos(
            &[
                "proto/neoutl/v1/common.proto",
                "proto/neoutl/v1/document.proto",
                "proto/neoutl/v1/settings.proto",
                "proto/neoutl/v1/keymap.proto",
                "proto/neoutl/v1/export.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
