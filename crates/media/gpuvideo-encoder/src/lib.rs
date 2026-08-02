//! H.264/H.265 Vulkan HWエンコーダプラグイン。gpuvideo-decoderと同一のVulkanDeviceを
//! 共有し、RGBA8UnormテクスチャをGPU内でNV12へ変換した上でエンコードする。
//! Vulkan非対応のmacOSでは無効化する。

#[cfg(not(target_os = "macos"))]
mod imp {
    use gpu_video::parameters::{
        ColorRange, ColorSpace, EncoderParametersH264, EncoderParametersH265, H265Profile,
        RateControl, VideoParameters, WgpuConverterParameters,
    };
    use gpu_video::{
        InputFrame, VulkanDevice as GpuVideoDevice, WgpuRgbaToNv12Converter,
        WgpuTexturesEncoderH264, WgpuTexturesEncoderH265,
    };
    use neoutl_media_api::{
        EncodeParameters, EncodedChunk, EncoderMeta, EncoderVTable, VideoCodec, VideoEncoder,
    };
    use std::sync::Arc;

    fn output_parameters_h264(
        device: &GpuVideoDevice,
        params: &EncodeParameters,
    ) -> Result<gpu_video::parameters::EncoderOutputParametersH264, String> {
        device
            .encoder_output_parameters_h264_high_quality(RateControl::VariableBitrate {
                average_bitrate: params.average_bitrate as u64,
                max_bitrate: params.max_bitrate as u64,
                virtual_buffer_size: std::time::Duration::from_secs(2),
            })
            .map_err(|e| e.to_string())
    }

    fn output_parameters_h265(
        device: &GpuVideoDevice,
        params: &EncodeParameters,
    ) -> Result<gpu_video::parameters::EncoderOutputParameters<H265Profile>, String> {
        device
            .encoder_output_parameters_h265_high_quality(RateControl::VariableBitrate {
                average_bitrate: params.average_bitrate as u64,
                max_bitrate: params.max_bitrate as u64,
                virtual_buffer_size: std::time::Duration::from_secs(2),
            })
            .map_err(|e| e.to_string())
    }

    /// RGBA8Unormテクスチャをエンコーダ入力解像度のNV12テクスチャへ変換する共通経路。
    struct Nv12Stage {
        converter: WgpuRgbaToNv12Converter,
        nv12_texture: wgpu::Texture,
    }

    impl Nv12Stage {
        fn new(device: &wgpu::Device, width: u32, height: u32) -> Result<Self, String> {
            let converter = WgpuRgbaToNv12Converter::new(
                device,
                WgpuConverterParameters {
                    color_space: ColorSpace::BT709,
                    color_range: ColorRange::Limited,
                },
            )
            .map_err(|e| e.to_string())?;
            let nv12_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("neoutl-encoder nv12 stage"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            Ok(Self {
                converter,
                nv12_texture,
            })
        }

        fn convert(
            &self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            rgba: &wgpu::Texture,
        ) -> wgpu::Texture {
            let y_view = self.nv12_texture.create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::Plane0,
                ..Default::default()
            });
            let uv_view = self.nv12_texture.create_view(&wgpu::TextureViewDescriptor {
                aspect: wgpu::TextureAspect::Plane1,
                ..Default::default()
            });
            let bind_group = self.converter.create_input_bind_group(rgba);
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            self.converter
                .convert(&mut encoder, &bind_group, &y_view, &uv_view);
            queue.submit(Some(encoder.finish()));
            self.nv12_texture.clone()
        }
    }

    pub struct GpuVideoEncoderH264 {
        encoder: WgpuTexturesEncoderH264,
        stage: Nv12Stage,
    }

    impl VideoEncoder for GpuVideoEncoderH264 {
        fn encode_rgba(
            &mut self,
            rgba: &wgpu::Texture,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            pts: i64,
            force_keyframe: bool,
        ) -> Result<Vec<EncodedChunk>, String> {
            let nv12 = self.stage.convert(device, queue, rgba);
            let chunk = self
                .encoder
                .encode(
                    InputFrame {
                        data: nv12,
                        pts: Some(pts as u64),
                    },
                    force_keyframe,
                )
                .map_err(|e| e.to_string())?;
            Ok(vec![EncodedChunk {
                data: chunk.data,
                pts,
                keyframe: chunk.is_keyframe,
            }])
        }

        fn flush(&mut self) -> Result<Vec<EncodedChunk>, String> {
            Ok(Vec::new())
        }
    }

    pub struct GpuVideoEncoderH265 {
        encoder: WgpuTexturesEncoderH265,
        stage: Nv12Stage,
    }

    impl VideoEncoder for GpuVideoEncoderH265 {
        fn encode_rgba(
            &mut self,
            rgba: &wgpu::Texture,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            pts: i64,
            force_keyframe: bool,
        ) -> Result<Vec<EncodedChunk>, String> {
            let nv12 = self.stage.convert(device, queue, rgba);
            let chunk = self
                .encoder
                .encode(
                    InputFrame {
                        data: nv12,
                        pts: Some(pts as u64),
                    },
                    force_keyframe,
                )
                .map_err(|e| e.to_string())?;
            Ok(vec![EncodedChunk {
                data: chunk.data,
                pts,
                keyframe: chunk.is_keyframe,
            }])
        }

        fn flush(&mut self) -> Result<Vec<EncodedChunk>, String> {
            Ok(Vec::new())
        }
    }

    static SHARED_DEVICE: std::sync::OnceLock<Arc<GpuVideoDevice>> = std::sync::OnceLock::new();

    /// main.rsが起動時に一度だけ呼ぶ。gpuvideo-decoder::set_shared_deviceと同一Arcを渡し、
    /// プロセス内Vulkanデバイスを単一に保つ。
    pub fn set_shared_device(device: Arc<GpuVideoDevice>) {
        let _ = SHARED_DEVICE.set(device);
    }

    fn shared_device() -> Result<&'static Arc<GpuVideoDevice>, String> {
        SHARED_DEVICE
            .get()
            .ok_or_else(|| "gpu_video共有デバイス未初期化".to_owned())
    }

    fn create_h264(params: EncodeParameters) -> Result<Box<dyn VideoEncoder>, String> {
        let device = shared_device()?;
        let width = std::num::NonZeroU32::new(params.width).ok_or("width=0")?;
        let height = std::num::NonZeroU32::new(params.height).ok_or("height=0")?;
        let output_parameters = output_parameters_h264(device, &params)?;
        let encoder = device
            .create_wgpu_textures_encoder_h264(EncoderParametersH264 {
                input_parameters: VideoParameters {
                    width,
                    height,
                    target_framerate: (params.framerate.round() as u32).into(),
                },
                output_parameters,
            })
            .map_err(|e| e.to_string())?;
        let stage = Nv12Stage::new(&device.wgpu_device(), params.width, params.height)?;
        Ok(Box::new(GpuVideoEncoderH264 { encoder, stage }))
    }

    fn create_h265(params: EncodeParameters) -> Result<Box<dyn VideoEncoder>, String> {
        let device = shared_device()?;
        let width = std::num::NonZeroU32::new(params.width).ok_or("width=0")?;
        let height = std::num::NonZeroU32::new(params.height).ok_or("height=0")?;
        let output_parameters = output_parameters_h265(device, &params)?;
        let encoder = device
            .create_wgpu_textures_encoder_h265(EncoderParametersH265 {
                input_parameters: VideoParameters {
                    width,
                    height,
                    target_framerate: (params.framerate.round() as u32).into(),
                },
                output_parameters,
            })
            .map_err(|e| e.to_string())?;
        let stage = Nv12Stage::new(&device.wgpu_device(), params.width, params.height)?;
        Ok(Box::new(GpuVideoEncoderH265 { encoder, stage }))
    }

    static META_H264: EncoderMeta = EncoderMeta {
        id: "neoutl.encoder.gpuvideo.h264",
        name: "GPU Video Encoder (H.264, Vulkan HW)",
        codec: VideoCodec::H264,
        hardware: true,
    };
    static META_H265: EncoderMeta = EncoderMeta {
        id: "neoutl.encoder.gpuvideo.h265",
        name: "GPU Video Encoder (H.265, Vulkan HW)",
        codec: VideoCodec::H265,
        hardware: true,
    };

    pub fn native_vtables() -> Vec<EncoderVTable> {
        vec![
            EncoderVTable {
                meta: || &META_H264,
                create: create_h264,
            },
            EncoderVTable {
                meta: || &META_H265,
                create: create_h265,
            },
        ]
    }
}

#[cfg(not(target_os = "macos"))]
pub use imp::*;

/// macOS向け無効化スタブ。Vulkan非対応のため空配列を返す。
#[cfg(target_os = "macos")]
pub fn native_vtables() -> Vec<neoutl_media_api::EncoderVTable> {
    Vec::new()
}
