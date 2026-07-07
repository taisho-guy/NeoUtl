use neoutl_media_api::AudioBuffer;
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::sample::Sample;
use symphonia::core::audio::{Audio, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

fn append_f32(buf: GenericAudioBufferRef<'_>, channels: usize, out: &mut Vec<f32>) {
    match buf {
        GenericAudioBufferRef::U8(b) => {
            interleave(&b, channels, out, |s| (s as f32 - 128.0) / 128.0)
        }
        GenericAudioBufferRef::U16(b) => {
            interleave(&b, channels, out, |s| (s as f32 - 32768.0) / 32768.0)
        }
        GenericAudioBufferRef::U24(b) => interleave(&b, channels, out, |s| {
            (s.0 as f32 - 8_388_608.0) / 8_388_608.0
        }),
        GenericAudioBufferRef::U32(b) => interleave(&b, channels, out, |s| {
            (s as f64 / u32::MAX as f64) as f32 * 2.0 - 1.0
        }),
        GenericAudioBufferRef::S8(b) => interleave(&b, channels, out, |s| s as f32 / 128.0),
        GenericAudioBufferRef::S16(b) => interleave(&b, channels, out, |s| s as f32 / 32768.0),
        GenericAudioBufferRef::S24(b) => {
            interleave(&b, channels, out, |s| s.0 as f32 / 8_388_608.0)
        }
        GenericAudioBufferRef::S32(b) => {
            interleave(&b, channels, out, |s| s as f32 / i32::MAX as f32)
        }
        GenericAudioBufferRef::F32(b) => interleave(&b, channels, out, |s| s),
        GenericAudioBufferRef::F64(b) => interleave(&b, channels, out, |s| s as f32),
    }
}

fn interleave<S: Copy + Sample, F: Fn(S) -> f32>(
    buf: &symphonia::core::audio::AudioBuffer<S>,
    channels: usize,
    out: &mut Vec<f32>,
    conv: F,
) {
    let frames = buf.frames();
    for i in 0..frames {
        for ch in 0..channels {
            out.push(conv(buf.plane(ch).unwrap()[i]));
        }
    }
}

pub fn decode_full(path: &Path) -> Result<AudioBuffer, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format: Box<dyn FormatReader> = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or("音声トラック未検出")?
        .clone();
    let track_id = track.id;
    let audio_cp = track
        .codec_params
        .as_ref()
        .and_then(|cp| cp.audio())
        .ok_or("codec_params未定義")?;
    let sample_rate = audio_cp.sample_rate.ok_or("sample_rate未定義")?;
    let channels = audio_cp.channels.as_ref().ok_or("channels未定義")?.count();

    let registry = symphonia::default::get_codecs();
    let mut decoder = registry
        .make_audio_decoder(audio_cp, &AudioDecoderOptions::default())
        .map_err(|e| e.to_string())?;

    let mut samples = Vec::new();
    loop {
        let packet = match format.next_packet().map_err(|e| e.to_string())? {
            Some(p) => p,
            None => break,
        };
        if packet.track_id != track_id {
            continue;
        }
        if let Ok(decoded) = decoder.decode(&packet) {
            append_f32(decoded, channels, &mut samples);
        }
    }

    Ok(AudioBuffer {
        sample_rate,
        channels: channels as u16,
        samples,
    })
}
