// src/ecs/systems.rs
use super::EcsWorld;
use crate::ecs::components::TextContent;
use crate::objects::RenderKind;

/// レンダラーに渡す 1 オブジェクト分の描画情報
pub struct ActiveObject {
    pub kind: RenderKind,
    /// テキストオブジェクトのみ Some
    pub text_content: Option<TextContent>,
}

/// 現在フレームでアクティブなオブジェクトを収集する
pub fn get_active_objects_system(world: &EcsWorld) -> Vec<ActiveObject> {
    let current = world.resources.current_frame;

    world
        .time_ranges
        .iter()
        .zip(world.render_kinds.iter())
        .zip(world.text_contents.iter())
        .filter_map(|((range, &kind), text)| {
            if current >= range.start_frame && current < range.end_frame {
                Some(ActiveObject {
                    kind,
                    text_content: text.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}
