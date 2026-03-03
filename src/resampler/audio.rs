use anyhow::Result;
use std::path::Path;

pub fn read_audio(path: &Path, target_sr: u32) -> Result<Vec<f64>> {
    let (mut audio, source_sr) = crate::utils::decode_wav_samples(path)?;

    if source_sr != target_sr {
        audio = resample_audio(&audio, source_sr, target_sr)?;
    }
    Ok(audio)
}

pub fn resample_audio(audio: &[f64], in_fs: u32, out_fs: u32) -> Result<Vec<f64>> {
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

    let mut resampler = SincFixedIn::<f64>::new(ratio, 2.0, params, 1024, 1)?;

    let mut padded = vec![0.0; 1024];

    for chunk in audio.chunks(1024) {
        let len = chunk.len();
        padded[..len].copy_from_slice(chunk);
        if len < 1024 {
            padded[len..].fill(0.0);
        }
        let res = resampler.process(&[&padded], None)?;
        resampled.extend_from_slice(&res[0]);
    }

    Ok(resampled)
}

pub fn write_audio(path: &Path, audio: &[f64], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let file = std::fs::File::create(path)?;
    let buf_writer = std::io::BufWriter::with_capacity(256 * 1024, file);
    let mut writer = hound::WavWriter::new(buf_writer, spec)?;
    let mut error_accum = 0.0_f64;
    let mut prng = crate::utils::XorShift32::new(0x12345678);
    for &x in audio {
        let scaled = x * 32767.0 + error_accum;

        let r1 = prng.next_f32() as f64;
        let r2 = prng.next_f32() as f64;
        let dither = r1 + r2;

        let q = (scaled + dither).round().clamp(-32768.0, 32767.0) as i16;
        error_accum = scaled - q as f64;
        writer.write_sample(q)?;
    }
    writer.finalize()?;
    Ok(())
}
