use slank::{ShaderStage, SlangShaderBuilder, SlangTarget};
use std::path::{Path, PathBuf};

/// オブジェクトクレートのbuild.rsから呼ぶ唯一の関数。
/// vs_main・fs_mainを単一.slangファイル内に完結させる形式（エフェクトと異なり共有頂点契約を持たない）を
/// WGSLへコンパイルし、OUT_DIR/{label}.wgslへ出力する。
pub fn compile_object_shader(label: &str, source_relpath: &str) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIRが未設定");
    let source_path = Path::new(&manifest_dir).join(source_relpath);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIRが未設定");
    let dest_path = PathBuf::from(&out_dir).join(format!("{label}.wgsl"));

    SlangShaderBuilder::new(label)
        .add_source_path(source_path.to_str().expect("source_pathが非UTF-8"))
        .expect("シェーダソースの読み込みに失敗")
        .entry_with_stage("vs_main", ShaderStage::Vertex)
        .entry_with_stage("fs_main", ShaderStage::Fragment)
        .build(SlangTarget::Wgsl)
        .expect("Slang→WGSLコンパイルに失敗")
        .output(&dest_path)
        .expect("WGSL出力に失敗");

    println!("cargo:rerun-if-changed={}", source_path.display());
}
