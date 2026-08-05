#![recursion_limit = "256"]

rust_i18n::i18n!("i18n");
#[macro_use]
extern crate rust_i18n;

mod app_state;
mod audio;
mod config;
mod document;
mod easings;
mod ecs;
mod effects;
mod egui_loop;
mod export;
mod gpu_shared;
mod hot_reload;
mod localization;
mod media;
mod objects;
mod project;
mod renderer;
mod shortcuts;
mod theme;
mod ui;

use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    localization::initialize();

    let gpu = Rc::new(gpu_shared::init_shared_gpu()?);
    let preview_slot = egui_loop::make_preview_slot();
    ui::install(gpu.clone(), preview_slot.clone());
    egui_loop::run(gpu, preview_slot)
}
