// src/media/audio.rs
use ffmpeg_next as ffmpeg;
use ffmpeg_next::software::resampling::Context as ResamplingContext;
use ffmpeg_next::util::format::sample::{Sample, Type as SampleType};
use ffmpeg_next::util::frame::Audio as AudioFrame;
use std::path::Path;

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

pub fn decode_full(path: &Path) -> Result<AudioBuffer, ffmpeg::Error> {
    let mut input = ffmpeg::format::input(path)?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or(ffmpeg::Error::StreamNotFound)?;
    let stream_index = stream.index();

    let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
    let mut decoder = context.decoder().audio()?;

    let out_rate = decoder.rate();
    let out_channels = decoder.channels();
    let out_layout = decoder.channel_layout();

    let mut resampler = ResamplingContext::get(
        decoder.format(),
        decoder.channel_layout(),
        decoder.rate(),
        Sample::F32(SampleType::Packed),
        out_layout,
        out_rate,
    )?;

    let mut samples: Vec<f32> = Vec::new();
    let mut decoded = AudioFrame::empty();
    let mut resampled = AudioFrame::empty();

    for (stream, packet) in input.packets() {
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet)?;
        while decoder.receive_frame(&mut decoded).is_ok() {
            resampler.run(&decoded, &mut resampled)?;
            append_planar_f32(&resampled, out_channels, &mut samples);
        }
    }
    decoder.send_eof()?;
    while decoder.receive_frame(&mut decoded).is_ok() {
        resampler.run(&decoded, &mut resampled)?;
        append_planar_f32(&resampled, out_channels, &mut samples);
    }

    Ok(AudioBuffer {
        sample_rate: out_rate,
        channels: out_channels,
        samples,
    })
}

fn append_planar_f32(frame: &AudioFrame, channels: u16, out: &mut Vec<f32>) {
    let data = frame.data(0);
    let sample_count = frame.samples() * channels as usize;
    let bytes = &data[..sample_count * 4];
    out.extend(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    );
}
