use std::ffi::c_void;

use crate::error::CarlaError;
use crate::host::CarlaHost;

#[cfg(feature = "egui")]
pub fn extract_raw_window_ptr(handle: raw_window_handle::RawWindowHandle) -> Option<*mut c_void> {
    match handle {
        #[cfg(target_os = "windows")]
        raw_window_handle::RawWindowHandle::Win32(w) => Some(w.hwnd.get() as usize as *mut c_void),
        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        raw_window_handle::RawWindowHandle::Xlib(w) => Some(w.window as *mut c_void),
        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        raw_window_handle::RawWindowHandle::Xcb(w) => Some(w.window.get() as *mut c_void),
        #[cfg(target_os = "macos")]
        raw_window_handle::RawWindowHandle::AppKit(w) => Some(w.ns_view.as_ptr() as *mut c_void),
        _ => None,
    }
}

#[cfg(feature = "egui")]
pub struct EmbeddedPluginUi {
    pub plugin_id: u32,
    pub title: String,
    pub is_embedded: bool,
    pub is_floating_open: bool,
    pub embedded_handle: *mut c_void,
    pub preferred_size: egui::Vec2,
    texture_handle: Option<egui::TextureHandle>,
}

#[cfg(feature = "egui")]
unsafe impl Send for EmbeddedPluginUi {}

#[cfg(feature = "egui")]
impl EmbeddedPluginUi {
    pub fn new(plugin_id: u32, title: impl Into<String>) -> Self {
        Self {
            plugin_id,
            title: title.into(),
            is_embedded: false,
            is_floating_open: false,
            embedded_handle: std::ptr::null_mut(),
            preferred_size: egui::vec2(400.0, 300.0),
            texture_handle: None,
        }
    }

    pub fn show_floating_window(&mut self, host: &CarlaHost) {
        host.show_custom_ui(self.plugin_id, true);
        self.is_floating_open = true;
    }

    pub fn hide_floating_window(&mut self, host: &CarlaHost) {
        host.show_custom_ui(self.plugin_id, false);
        self.is_floating_open = false;
    }

    pub fn toggle_floating_window(&mut self, host: &CarlaHost) {
        if self.is_floating_open {
            self.hide_floating_window(host);
        } else {
            self.show_floating_window(host);
        }
    }

    pub fn embed_into(
        &mut self,
        host: &CarlaHost,
        parent_window_ptr: *mut c_void,
    ) -> Result<*mut c_void, CarlaError> {
        if parent_window_ptr.is_null() {
            return Err(CarlaError::NullPointer);
        }

        let ptr = host.embed_custom_ui(self.plugin_id, parent_window_ptr);
        if !ptr.is_null() {
            self.is_embedded = true;
            self.embedded_handle = ptr;
            Ok(ptr)
        } else {
            Err(CarlaError::OperationFailed(
                "Carla embed_custom_ui returned null pointer".to_string(),
            ))
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        host: &CarlaHost,
        parent_window: Option<*mut c_void>,
    ) -> egui::Response {
        let plugin_info = host.plugin_info(self.plugin_id);
        let has_custom_ui = plugin_info
            .as_ref()
            .map_or(false, |info| info.has_custom_ui());
        let can_embed = plugin_info
            .as_ref()
            .map_or(false, |info| info.can_embed_custom_ui());
        let has_inline = plugin_info
            .as_ref()
            .map_or(false, |info| info.has_inline_display());

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let name = plugin_info
                    .as_ref()
                    .map(|i| i.name.as_str())
                    .unwrap_or(&self.title);
                ui.heading(name);

                if has_custom_ui {
                    let btn_text = if self.is_floating_open {
                        "🗗 Hide Window"
                    } else {
                        "🗖 Open Window"
                    };

                    if ui.button(btn_text).clicked() {
                        self.toggle_floating_window(host);
                    }
                }
            });

            ui.separator();

            if can_embed && parent_window.is_some() && !self.is_embedded {
                if let Some(parent_ptr) = parent_window {
                    let _ = self.embed_into(host, parent_ptr);
                }
            }

            if self.is_embedded {
                let (rect, response) =
                    ui.allocate_exact_size(self.preferred_size, egui::Sense::hover());
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
                    egui::StrokeKind::Inside,
                );
                response
            } else if has_inline {
                let w = self.preferred_size.x.max(64.0) as u32;
                let h = self.preferred_size.y.max(64.0) as u32;

                if let Some(surface) = host.render_inline_display(self.plugin_id, w, h) {
                    let color_image = surface.to_color_image();
                    let tex = self.texture_handle.get_or_insert_with(|| {
                        ui.ctx().load_texture(
                            format!("carla_inline_{}", self.plugin_id),
                            color_image.clone(),
                            egui::TextureOptions::LINEAR,
                        )
                    });
                    tex.set(color_image, egui::TextureOptions::LINEAR);

                    ui.image((tex.id(), egui::vec2(w as f32, h as f32)))
                } else {
                    self.render_fallback_card(ui, host, &plugin_info, has_custom_ui)
                }
            } else {
                self.render_fallback_card(ui, host, &plugin_info, has_custom_ui)
            }
        })
        .inner
    }

    fn render_fallback_card(
        &mut self,
        ui: &mut egui::Ui,
        host: &CarlaHost,
        plugin_info: &Option<crate::types::PluginInfo>,
        has_custom_ui: bool,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(self.preferred_size, egui::Sense::click());

        ui.painter()
            .rect_filled(rect, 6.0, ui.visuals().extreme_bg_color);
        ui.painter().rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );

        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)));
        child_ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            if let Some(info) = plugin_info {
                ui.label(egui::RichText::new(&info.name).strong().size(16.0));
                if !info.maker.is_empty() {
                    ui.label(egui::RichText::new(format!("by {}", info.maker)).weak());
                }
                ui.add_space(8.0);
                ui.label(format!("Type: {:?}", info.plugin_type));
                ui.label(format!("Category: {:?}", info.category));
            } else {
                ui.label(egui::RichText::new(&self.title).strong().size(16.0));
            }

            ui.add_space(16.0);

            if has_custom_ui {
                let btn_text = if self.is_floating_open {
                    "🗗 Hide Plugin GUI Window"
                } else {
                    "🗖 Open Plugin GUI Window"
                };

                if ui
                    .button(egui::RichText::new(btn_text).size(14.0))
                    .clicked()
                {
                    self.toggle_floating_window(host);
                }
            } else {
                ui.label(egui::RichText::new("No dedicated custom GUI (parameters only)").weak());
            }
        });

        response
    }
}
