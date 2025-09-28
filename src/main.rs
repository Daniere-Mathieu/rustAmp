use std::{
    error::Error,
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

fn main() -> Result<(), Box<dyn Error>> {
    let file_path = std::env::args()
        .nth(1)
        .ok_or("Usage: cargo run -- <audio-file>")?;

    // === Decode the entire file into interleaved stereo samples ===
    let samples = Arc::new(Mutex::new(load_samples(&file_path)?));
    let total_samples = samples.lock().unwrap().len();

    // === Set up CPAL output ===
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("No output device")?;
    let supported = device.default_output_config()?;
    let mut config = supported.config();
    config.channels = 2; // we output stereo (seem to allow to have the sound from each side)

    let mut index = 0usize;
    let samples_for_cb = Arc::clone(&samples);

    let err_fn = |e| eprintln!("Stream error: {e}");

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            let s = samples_for_cb.lock().unwrap();
            for frame in data.iter_mut() {
                if index < total_samples {
                    *frame = s[index];
                    index += 1;
                } else {
                    *frame = 0.0; // silence after EOF
                }
            }
        },
        err_fn,
        None,
    )?;

    stream.play()?;

    // Keep main thread alive while the file plays.
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));

        if index >= total_samples {
            break;
        }
    }
    Ok(())
}

/// Decode the whole file to a single Vec<f32> (stereo interleaved).
fn load_samples(path: &str) -> Result<Vec<f32>, Box<dyn Error>> {
    let file = File::open(Path::new(path))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("No supported audio tracks")?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions { verify: false })?;

    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut out = Vec::<f32>::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(e) => return Err(Box::<dyn Error>::from(e.to_string())),
        };

        let decoded = match decoder.decode(&packet) {
            Ok(buf) => buf,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(Box::<dyn Error>::from(e.to_string())),
        };

        let spec = *decoded.spec();
        let capacity = decoded.capacity() as u64;
        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::<f32>::new(capacity, spec));
        }
        let sbuf = sample_buf.as_mut().unwrap();
        sbuf.copy_interleaved_ref(decoded);

        let ch = spec.channels.count() as usize;
        match ch {
            1 => {
                // mono → duplicate to stereo
                for s in sbuf.samples() {
                    out.push(*s);
                    out.push(*s);
                }
            }
            2 => out.extend_from_slice(sbuf.samples()),
            _ => {
                // more than 2ch → take first two
                for frame in sbuf.samples().chunks_exact(ch) {
                    out.push(frame[0]);
                    out.push(frame[1]);
                }
            }
        }
    }

    Ok(out)
}
