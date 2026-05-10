use anyhow::Result;
use std::path::Path;

pub fn read_audio(path: &Path, target_sr: u32) -> Result<Vec<f32>> {
    let (mut audio, source_sr) = crate::utils::decode_wav_samples(path)?;

    if source_sr != target_sr {
        audio = resample_audio(&audio, source_sr, target_sr)?;
    }
    Ok(audio)
}

pub fn resample_audio(audio: &[f32], in_fs: u32, out_fs: u32) -> Result<Vec<f32>> {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    let ratio = out_fs as f64 / in_fs as f64;
    let out_samples = (audio.len() as f64 * ratio) as usize;
    let mut resampled = Vec::with_capacity(out_samples);

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 1.0,
        oversampling_factor: 128,
        interpolation: SincInterpolationType::Cubic,
        window: WindowFunction::Hann,
    };

    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, 1024, 1)?;
    let mut input = resampler.input_buffer_allocate(true);
    let mut output = resampler.output_buffer_allocate(true);
    let mut offset = 0usize;

    while offset < audio.len() {
        let frames_in = resampler.input_frames_next();
        let end = (offset + frames_in).min(audio.len());
        let chunk = &audio[offset..end];
        input[0][..chunk.len()].copy_from_slice(chunk);
        if chunk.len() < frames_in {
            input[0][chunk.len()..frames_in].fill(0.0);
        }

        let (_, out_len) = resampler.process_into_buffer(&input, &mut output, None)?;
        resampled.extend_from_slice(&output[0][..out_len]);
        offset = end;
    }

    Ok(resampled)
}

pub fn write_audio(
    path: &Path,
    audio: &[f32],
    sample_rate: u32,
    output_dither: bool,
) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let file = std::fs::File::create(path)?;
    let buf_writer = std::io::BufWriter::with_capacity(256 * 1024, file);
    let mut writer = hound::WavWriter::new(buf_writer, spec)?;
    const WRITE_CHUNK_SAMPLES: usize = 8192;
    if output_dither {
        let mut error_accum = 0.0_f32;
        let mut prng = crate::utils::XorShift32::new(0x12345678);
        for chunk in audio.chunks(WRITE_CHUNK_SAMPLES) {
            let mut sample_writer = writer.get_i16_writer(chunk.len() as u32);
            for &x in chunk {
                let scaled = x * 32767.0 + error_accum;

                let r1 = prng.next_f32();
                let r2 = prng.next_f32();
                let dither = r1 + r2;

                let q = (scaled + dither).round().clamp(-32768.0, 32767.0) as i16;
                error_accum = scaled - q as f32;
                sample_writer.write_sample(q);
            }
            sample_writer.flush()?;
        }
    } else {
        for chunk in audio.chunks(WRITE_CHUNK_SAMPLES) {
            let mut sample_writer = writer.get_i16_writer(chunk.len() as u32);
            for &x in chunk {
                let q = (x * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
                sample_writer.write_sample(q);
            }
            sample_writer.flush()?;
        }
    }
    writer.finalize()?;
    Ok(())
}
