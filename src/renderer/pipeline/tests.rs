use super::*;
use crate::ecs::resources::ProjectResource;

fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::None,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

fn read_texture_rgba16f(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<f32> {
    let unpadded_bytes_per_row = width * 8;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Test Readback Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    crate::gpu_shared::locked_submit(queue, [encoder.finish()]);

    let slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).expect(&t!("map_async結果送信失敗"));
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect(&t!("device poll失敗"));
    rx.recv()
        .expect(&t!("map_async結果受信失敗"))
        .expect(&t!("バッファmap失敗"));

    let padded = slice.get_mapped_range().expect(&t!("get_mapped_range失敗"));
    let mut dense = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
    for row in 0..height as usize {
        let start = row * padded_bytes_per_row as usize;
        let end = start + unpadded_bytes_per_row as usize;
        dense.extend_from_slice(&padded[start..end]);
    }
    drop(padded);
    output_buffer.unmap();
    dense
        .chunks_exact(2)
        .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
        .collect()
}

#[test]
fn render_engine_new_succeeds() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
        return;
    };
    let engine = RenderEngine::new(device, queue, 64, 64);
    assert_eq!(engine.render_width, 64);
    assert_eq!(engine.render_height, 64);
}

#[test]
fn render_empty_scene_clears_target() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
        return;
    };
    let mut engine = RenderEngine::new(device, queue, 32, 32);
    let project = ProjectResource::new();
    let world = crate::ecs::EcsWorld::new();
    let captured = std::collections::HashMap::new();
    engine.render(&world, &[], &captured, &project);

    let pixels = read_texture_rgba16f(
        &engine.device,
        &engine.queue,
        &engine.texture,
        engine.render_width,
        engine.render_height,
    );
    assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
    let alpha_values: Vec<f32> = pixels.iter().skip(3).step_by(4).copied().collect();
    assert!(alpha_values.iter().all(|&a| a == alpha_values[0]));
}

fn make_active_object(
    kind_id: u32,
    effects: Vec<(String, HashMap<String, Value>)>,
) -> ActiveObject {
    ActiveObject {
        kind_id,
        source_frame: 0,
        clip_instance: kind_id as u64,
        text_content: None,
        shape_params: None,
        media_source: None,
        mvp: [0.0; 16],
        opacity: 1.0,
        effects,
        compose_source: None,
        layer: 0,
        clip_target: None,
    }
}

#[test]
fn effect_chain_does_not_leak_to_adjacent_object() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
        return;
    };
    let mut engine = RenderEngine::new(device, queue, 32, 32);
    let project = ProjectResource::new();
    let world = crate::ecs::EcsWorld::new();

    let plain = make_active_object(u32::MAX, Vec::new());
    let with_effect = make_active_object(
        u32::MAX,
        vec![("nonexistent-effect-id".to_string(), HashMap::new())],
    );
    let captured = std::collections::HashMap::new();
    engine.render(&world, &[plain, with_effect], &captured, &project);

    let pixels = read_texture_rgba16f(
        &engine.device,
        &engine.queue,
        &engine.texture,
        engine.render_width,
        engine.render_height,
    );
    assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
}

#[test]
fn distinct_effect_chains_render_independently() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
        return;
    };
    let mut engine = RenderEngine::new(device, queue, 32, 32);
    let project = ProjectResource::new();
    let world = crate::ecs::EcsWorld::new();

    let obj_a = make_active_object(u32::MAX, vec![("effect-a".to_string(), HashMap::new())]);
    let obj_b = make_active_object(u32::MAX, vec![("effect-b".to_string(), HashMap::new())]);
    let captured = std::collections::HashMap::new();
    engine.render(&world, &[obj_a, obj_b], &captured, &project);

    let pixels = read_texture_rgba16f(
        &engine.device,
        &engine.queue,
        &engine.texture,
        engine.render_width,
        engine.render_height,
    );
    assert_eq!(pixels.len(), (32 * 32 * 4) as usize);
}

#[test]
fn resize_render_target_updates_dimensions_and_survives_render() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("{}", t!("[test] GPUアダプタ非検出、テストskip"));
        return;
    };
    let mut engine = RenderEngine::new(device, queue, 64, 64);
    engine.resize_render_target(128, 72);
    assert_eq!(engine.render_width, 128);
    assert_eq!(engine.render_height, 72);

    let project = ProjectResource::new();
    let world = crate::ecs::EcsWorld::new();
    let captured = std::collections::HashMap::new();
    engine.render(&world, &[], &captured, &project);
    let pixels = read_texture_rgba16f(
        &engine.device,
        &engine.queue,
        &engine.texture,
        engine.render_width,
        engine.render_height,
    );
    assert_eq!(pixels.len(), (128 * 72 * 4) as usize);
}
