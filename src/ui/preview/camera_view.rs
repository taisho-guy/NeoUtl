use super::interaction::InteractionState;
use crate::app::state::SharedAppState;
use crate::ecs::EcsWorld;
use crate::ecs::systems::get_active_objects_system;
use crate::ecs::transform::Camera;
use egui::{Color32, Pos2, Rect, Stroke, TextureId, Vec2};
use shipyard::UniqueView;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CameraViewMode {
    Camera,
    Edit,
    Front,
    Left,
    Top,
    Back,
    Right,
    Bottom,
}

impl CameraViewMode {
    const ALL: [Self; 8] = [
        Self::Camera,
        Self::Edit,
        Self::Front,
        Self::Left,
        Self::Top,
        Self::Back,
        Self::Right,
        Self::Bottom,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Camera => "カメラ",
            Self::Edit => "エディット",
            Self::Front => "前",
            Self::Left => "左",
            Self::Top => "上",
            Self::Back => "後",
            Self::Right => "右",
            Self::Bottom => "下",
        }
    }
}

pub fn show(
    ctx: &egui::Context,
    open: &mut bool,
    mode: &mut CameraViewMode,
    texture_id: Option<TextureId>,
    world: &EcsWorld,
    interaction: &mut InteractionState,
    state: &SharedAppState,
) {
    egui::Window::new("カメラ視点")
        .open(open)
        .default_size([520.0, 360.0])
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for candidate in CameraViewMode::ALL {
                    if ui
                        .selectable_label(*mode == candidate, candidate.label())
                        .clicked()
                    {
                        *mode = candidate;
                    }
                }
            });
            ui.separator();

            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
            interaction.handle(&response, response.rect, state);
            if *mode == CameraViewMode::Camera {
                if let Some(texture_id) = texture_id {
                    painter.image(
                        texture_id,
                        response.rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            } else {
                draw_edit_view(&painter, response.rect, world, *mode);
            }
        });
}

fn draw_edit_view(painter: &egui::Painter, rect: Rect, world: &EcsWorld, mode: CameraViewMode) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 14, 18));
    let center = rect.center();
    let scale = (rect.width().min(rect.height()) / 900.0).max(0.2);
    let grid_color = Color32::from_rgba_unmultiplied(150, 160, 175, 75);

    for index in -10..=10 {
        let offset = index as f32 * 40.0 * scale;
        painter.line_segment(
            [
                Pos2::new(center.x + offset, rect.top()),
                Pos2::new(center.x + offset, rect.bottom()),
            ],
            Stroke::new(1.0, grid_color),
        );
        painter.line_segment(
            [
                Pos2::new(rect.left(), center.y + offset),
                Pos2::new(rect.right(), center.y + offset),
            ],
            Stroke::new(1.0, grid_color),
        );
    }

    let camera = world.world.run(|camera: UniqueView<Camera>| *camera);
    let project = world.get_project();
    let (active, _) = get_active_objects_system(world);
    let object_color = Color32::from_rgb(220, 225, 235);
    for object in active {
        let Some(transform) = world.get_transform(object.clip_instance as usize) else {
            continue;
        };
        let position = view_position(mode, transform.x, transform.y, transform.z, center, scale);
        painter.circle_filled(position, 4.0, object_color);
    }

    let camera_pos = view_position(
        mode,
        camera.pos_x,
        camera.pos_y,
        camera.pos_z,
        center,
        scale,
    );
    let target_pos = view_position(
        mode,
        camera.target_x,
        camera.target_y,
        camera.target_z,
        center,
        scale,
    );
    painter.circle_filled(camera_pos, 6.0, Color32::from_rgb(245, 80, 80));
    painter.line_segment(
        [camera_pos, target_pos],
        Stroke::new(2.0, Color32::from_rgb(245, 80, 80)),
    );
    painter.circle_filled(target_pos, 4.0, Color32::from_rgb(245, 210, 70));
    painter.text(
        rect.left_top() + Vec2::splat(10.0),
        egui::Align2::LEFT_TOP,
        format!("{}  {}x{}", mode.label(), project.width, project.height),
        egui::FontId::monospace(12.0),
        Color32::from_rgb(210, 215, 225),
    );
}

fn view_position(mode: CameraViewMode, x: f32, y: f32, z: f32, center: Pos2, scale: f32) -> Pos2 {
    let (horizontal, vertical) = match mode {
        CameraViewMode::Left | CameraViewMode::Right => (z, y),
        CameraViewMode::Top | CameraViewMode::Bottom => (x, z),
        _ => (x, y),
    };
    let horizontal = if matches!(
        mode,
        CameraViewMode::Back | CameraViewMode::Right | CameraViewMode::Bottom
    ) {
        -horizontal
    } else {
        horizontal
    };
    Pos2::new(center.x + horizontal * scale, center.y - vertical * scale)
}
