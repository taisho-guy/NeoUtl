use slank::{ShaderStage, SlangShaderBuilder, SlangTarget};
use std::path::{Path, PathBuf};

/// エフェクトクレートのbuild.rsから呼ぶ唯一の関数。
/// neoutl-effect-api/slang/effect_prelude.slang（vs_main・input_tex・input_sampler契約）を
/// fragment_path（fs_main本体）へ前段連結してWGSLへコンパイルし、OUT_DIR/{label}.wgslへ出力する。
/// 呼び出し元crate::slank::include_slang!(label)がこのファイルを読み込む。
pub fn compile_effect_fragment(label: &str, fragment_relpath: &str) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIRが未設定");
    let prelude_path =
        Path::new(&manifest_dir).join("../../neoutl-effect-api/slang/effect_prelude.slang");
    let fragment_path = Path::new(&manifest_dir).join(fragment_relpath);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIRが未設定");
    let dest_path = PathBuf::from(&out_dir).join(format!("{label}.wgsl"));

    SlangShaderBuilder::new(label)
        .add_source_path(prelude_path.to_str().expect("prelude_pathが非UTF-8"))
        .expect("effect_prelude.slangの読み込みに失敗")
        .add_source_path(fragment_path.to_str().expect("fragment_pathが非UTF-8"))
        .expect("fragmentシェーダの読み込みに失敗")
        .entry_with_stage("vs_main", ShaderStage::Vertex)
        .entry_with_stage("fs_main", ShaderStage::Fragment)
        .build(SlangTarget::Wgsl)
        .expect("Slang→WGSLコンパイルに失敗")
        .output(&dest_path)
        .expect("WGSL出力に失敗");

    println!("cargo:rerun-if-changed={}", prelude_path.display());
    println!("cargo:rerun-if-changed={}", fragment_path.display());
}
