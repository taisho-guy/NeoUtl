// src/macos.rs
// 対応するlib.rs側の変更（必須）:
//   #[cfg(target_os = "macos")]
//   const ZEROCOPY_CAPS: &str = "video/x-raw,format=NV12"; // (旧) "video/x-raw(memory:GLMemory),format=NV12"
use gst::prelude::*;
use gstreamer as gst;

/// macOS向け映像フレーム取り込み。
/// CVPixelBuffer/IOSurfaceをGstGLMemoryから取得する公開APIはgstreamer-gl・objc2-core-video
/// いずれのcrateにも存在しないため（該当機能はGStreamerのapplemedia C実装内部限定）、
/// NV12バッファをCPU経由でwgpuテクスチャへ転送する。
pub unsafe fn import_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &gst::BufferRef,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    let map = buffer.map_readable().map_err(|e| e.to_string())?;
    let data = map.as_slice();

    let y_plane_size = (width * height) as usize;
    let uv_plane_size = (width * height / 2) as usize;
    if data.len() < y_plane_size + uv_plane_size {
        return Err("NV12バッファサイズ不足".to_owned());
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gst-nv12-frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::NV12,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane0,
        },
        &data[0..y_plane_size],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane1,
        },
        &data[y_plane_size..y_plane_size + uv_plane_size],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height / 2),
        },
        wgpu::Extent3d {
            width: width / 2,
            height: height / 2,
            depth_or_array_layers: 1,
        },
    );

    Ok(texture)
}
