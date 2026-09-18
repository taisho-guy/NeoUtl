use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use std::sync::Arc;
use winit::event_loop::EventLoop;
#[cfg(target_os = "linux")]
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::window::WindowAttributes;

pub fn extract_raw_window_handle<W: HasWindowHandle>(window: &W) -> *mut c_void {
    if let Ok(handle) = window.window_handle() {
        match handle.as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as *mut c_void,
            RawWindowHandle::Xlib(h) => h.window as *mut c_void,
            RawWindowHandle::Xcb(h) => h.window.get() as *mut c_void,
            RawWindowHandle::AppKit(h) => h.ns_view.as_ptr(),
            _ => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn launch_winit_wgpu_layer() -> *mut c_void {
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "linux")]
    builder.with_x11();

    let event_loop = match builder.build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("[Winit] EventLoop build failed: {e}");
            return std::ptr::null_mut();
        }
    };

    let attributes = WindowAttributes::default()
        .with_visible(false)
        .with_title("NeoUtl Render Layer");

    let window = match event_loop.create_window(attributes) {
        Ok(w) => Arc::new(w),
        Err(e) => {
            eprintln!("[Winit] Window create failed: {e}");
            return std::ptr::null_mut();
        }
    };

    let raw_handle = extract_raw_window_handle(&*window);

    let instance = wgpu::Instance::default();
    let target = window.clone();
    let surface = match instance.create_surface(target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[wgpu] create_surface failed: {e}");
            return raw_handle;
        }
    };

    let render_window = window.clone();
    std::thread::Builder::new()
        .name("neo-render-thread".into())
        .spawn(move || {
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                    apply_limit_buckets: false,
                }));

            let adapter = match adapter {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("[wgpu] request_adapter failed: {e}");
                    return;
                }
            };

            let (device, queue) =
                match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("neoqtl-wgpu-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    ..Default::default()
                })) {
                    Ok(dq) => dq,
                    Err(e) => {
                        eprintln!("[wgpu] request_device failed: {e}");
                        return;
                    }
                };

            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .first()
                .copied()
                .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
            let mut width = 1280u32;
            let mut height = 720u32;

            let mut config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width,
                height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);

            loop {
                let cur_size = render_window.inner_size();
                if cur_size.width > 0
                    && cur_size.height > 0
                    && (cur_size.width != width || cur_size.height != height)
                {
                    width = cur_size.width;
                    height = cur_size.height;
                    config.width = width;
                    config.height = height;
                    surface.configure(&device, &config);
                }

                let frame = match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    _ => {
                        surface.configure(&device, &config);
                        std::thread::sleep(std::time::Duration::from_millis(16));
                        continue;
                    }
                };

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("neoqtl-render-encoder"),
                });

                {
                    let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("neoqtl-clear-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.11,
                                    g: 0.11,
                                    b: 0.12,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                }

                queue.submit(std::iter::once(encoder.finish()));
                queue.present(frame);

                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        })
        .ok();

    raw_handle
}
