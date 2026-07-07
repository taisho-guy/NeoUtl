pub trait VideoSource: Send {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn fps(&self) -> f64;
    fn total_frames(&self) -> i64;
    fn frame_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_index: i64,
    ) -> Result<wgpu::Texture, String>;
}

pub trait ImageSource: Send {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture;
}

pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn range(&self, start_sample: usize, sample_count: usize) -> &[f32] {
        let channels = self.channels.max(1) as usize;
        let start = start_sample
            .saturating_mul(channels)
            .min(self.samples.len());
        let end = (start_sample + sample_count)
            .saturating_mul(channels)
            .min(self.samples.len());
        &self.samples[start..end]
    }
}

pub trait AudioSource {
    fn decode_full(path: &std::path::Path) -> Result<AudioBuffer, String>;
}
