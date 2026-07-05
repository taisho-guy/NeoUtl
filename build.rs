// build.rs
use std::{collections::HashMap, path::PathBuf};

fn main() {
    let library = HashMap::from([("lucide".to_string(), PathBuf::from(lucide_slint::lib()))]);
    let config = slint_build::CompilerConfiguration::new().with_library_paths(library);
    slint_build::compile_with_config("src/app.slint", config).expect("Slint build failed");
}
