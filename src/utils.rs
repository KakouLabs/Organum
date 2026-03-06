pub struct XorShift32(pub u32);

impl XorShift32 {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }

    #[inline(always)]
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) - 0.5
    }
}

/// "C4" -> 60, "A#3" -> 58 등. 파싱 실패 시 60 반환.
pub fn note_to_midi(note: &str) -> i32 {
    let note = note.trim();
    if note.is_empty() {
        return 60;
    }

    let mut chars = note.chars();
    let mut step_str = String::new();
    let mut octave_str = String::new();
    let mut is_sharp = false;
    let mut is_flat = false;

    if let Some(c) = chars.next() {
        step_str.push(c.to_ascii_uppercase());
    }

    for c in chars {
        if c == '#' {
            is_sharp = true;
        } else if c == 'b' {
            is_flat = true;
        } else if c.is_ascii_digit() || c == '-' {
            octave_str.push(c);
        }
    }

    let octave: i32 = octave_str.parse().unwrap_or(4);

    let base_step = match step_str.as_str() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => 9,
    };

    let mut semitone = base_step;
    if is_sharp {
        semitone += 1;
    }
    if is_flat {
        semitone -= 1;
    }

    (octave + 1) * 12 + semitone
}

/// MIDI -> Hz. `440 * 2^((midi - 69) / 12)`
#[inline(always)]
pub fn midi_to_hz(midi: f64) -> f64 {
    440.0 * ((midi - 69.0) / 12.0).exp2()
}

pub fn note_to_freq(note: &str) -> f64 {
    midi_to_hz(note_to_midi(note) as f64)
}

pub fn decode_wav_samples(path: &std::path::Path) -> anyhow::Result<(Vec<f64>, u32)> {
    use anyhow::Context;

    let mut reader =
        hound::WavReader::open(path).context(format!("Failed to open WAV: {:?}", path))?;
    let spec = reader.spec();
    let max_val: f64 = match spec.bits_per_sample {
        8 => 128.0,
        16 => 32768.0,
        24 => 8388608.0,
        32 => 2147483648.0,
        _ => 32768.0,
    };
    let channels = spec.channels as usize;
    let total_samples = reader.len() as usize;
    let estimated_frames = total_samples / channels.max(1);

    let mut mono: Vec<f64> = Vec::with_capacity(estimated_frames);

    if channels <= 1 {
        // 모노: 직접 정규화하면서 수집
        for (i, s) in reader.samples::<i32>().enumerate() {
            let sample = s.unwrap_or_else(|e| {
                tracing::warn!("Corrupted sample at index {} in {:?}: {}", i, path, e);
                0
            });
            mono.push(sample as f64 / max_val);
        }
    } else {
        let inv_ch = 1.0 / (channels as f64 * max_val);
        let mut ch_sum: f64 = 0.0;
        let mut ch_idx: usize = 0;

        for (i, s) in reader.samples::<i32>().enumerate() {
            let sample = s.unwrap_or_else(|e| {
                tracing::warn!("Corrupted sample at index {} in {:?}: {}", i, path, e);
                0
            });
            ch_sum += sample as f64;
            ch_idx += 1;

            if ch_idx == channels {
                mono.push(ch_sum * inv_ch);
                ch_sum = 0.0;
                ch_idx = 0;
            }
        }
    }

    Ok((mono, spec.sample_rate))
}

pub fn decode_utau_char(c: char) -> i32 {
    match c {
        'A'..='Z' => (c as i32) - ('A' as i32),
        'a'..='z' => (c as i32) - ('a' as i32) + 26,
        '0'..='9' => (c as i32) - ('0' as i32) + 52,
        '+' => 62,
        '/' => 63,
        _ => -1,
    }
}

pub fn parse_pitchbend(tempo_pb: &str, pitchbend_data: &str) -> (f32, Vec<i16>) {
    let tempo: f32 = tempo_pb.trim_start_matches('!').parse().unwrap_or(120.0);

    if pitchbend_data.is_empty() {
        return (tempo, vec![]);
    }

    let mut result: Vec<i16> = Vec::new();
    let chars: Vec<char> = pitchbend_data.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '#' && i + 1 < chars.len() {
            let mut num_str = String::new();
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '#' {
                num_str.push(chars[j]);
                j += 1;
            }
            if j < chars.len() && chars[j] == '#' {
                if let Ok(count) = num_str.parse::<usize>() {
                    if let Some(&last) = result.last() {
                        for _ in 1..count {
                            result.push(last);
                        }
                    }
                }
                i = j + 1;
                continue;
            }
        }

        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];

            let v1 = decode_utau_char(c1);
            let v2 = decode_utau_char(c2);

            if v1 >= 0 && v2 >= 0 {
                let raw = v1 * 64 + v2;
                let value = if raw >= 2048 { raw - 4096 } else { raw } as i16;
                result.push(value);
            }
            i += 2;
        } else {
            i += 1;
        }
    }

    (tempo, result)
}

pub fn parse_utau_length(raw: &str, bpm_hint: f32) -> f32 {
    if raw.contains('@') {
        let parts: Vec<&str> = raw.split('@').collect();
        if parts.len() == 2 {
            let ticks: f32 = parts[0].parse().unwrap_or(0.0);

            let bpm_part = parts[1];
            let (bpm_str, offset) = if let Some(pos) = bpm_part.find('+') {
                let (b, o) = bpm_part.split_at(pos);
                (b, o[1..].parse::<f32>().unwrap_or(0.0))
            } else if let Some(pos) = bpm_part.find('-') {
                let (b, o) = bpm_part.split_at(pos);
                (b, -o[1..].parse::<f32>().unwrap_or(0.0))
            } else {
                (bpm_part, 0.0)
            };

            let bpm: f32 = bpm_str.parse().unwrap_or(bpm_hint);
            let ticks_per_beat = 480.0;
            let ms_per_beat = 60000.0 / bpm;
            let duration_ms = (ticks / ticks_per_beat) * ms_per_beat;

            return (duration_ms + offset).max(0.0);
        }
    }
    raw.parse().unwrap_or(0.0)
}

macro_rules! impl_cubic_interpolate {
    ($name:ident, $t:ty) => {
        #[inline(always)]
        pub fn $name(samples: &[$t], idx: $t) -> $t {
            let len = samples.len();
            if len == 0 {
                return 0.0;
            }
            if len == 1 {
                return samples[0];
            }

            let i = idx as isize;
            let frac = idx - i as $t;

            if i >= 1 && i < (len as isize - 2) {
                let p0 = unsafe { *samples.get_unchecked((i - 1) as usize) };
                let p1 = unsafe { *samples.get_unchecked(i as usize) };
                let p2 = unsafe { *samples.get_unchecked(i as usize + 1) };
                let p3 = unsafe { *samples.get_unchecked(i as usize + 2) };

                let t = frac;
                let t2 = t * t;
                let t3 = t2 * t;
                let a0 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
                let a1 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
                let a2 = -0.5 * p0 + 0.5 * p2;
                let a3 = p1;

                return a0 * t3 + a1 * t2 + a2 * t + a3;
            }

            let p0 = samples[(i - 1).clamp(0, len as isize - 1) as usize];
            let p1 = samples[i.clamp(0, len as isize - 1) as usize];
            let p2 = samples[(i + 1).clamp(0, len as isize - 1) as usize];
            let p3 = samples[(i + 2).clamp(0, len as isize - 1) as usize];

            let t = frac;
            let t2 = t * t;
            let t3 = t2 * t;

            let a0 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
            let a1 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
            let a2 = -0.5 * p0 + 0.5 * p2;
            let a3 = p1;

            a0 * t3 + a1 * t2 + a2 * t + a3
        }
    };
}

impl_cubic_interpolate!(cubic_interpolate_f32, f32);
impl_cubic_interpolate!(cubic_interpolate_f64, f64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_to_midi() {
        assert_eq!(note_to_midi("C4"), 60);
        assert_eq!(note_to_midi("A4"), 69);
        assert_eq!(note_to_midi("C#4"), 61);
        assert_eq!(note_to_midi("Bb4"), 70);
        assert_eq!(note_to_midi("G-1"), 7);
        assert_eq!(note_to_midi(" "), 60); // empty fallback
    }

    #[test]
    fn test_midi_to_hz() {
        assert!((midi_to_hz(69.0) - 440.0).abs() < 1e-5);
        assert!((midi_to_hz(60.0) - 261.625565).abs() < 1e-4);
    }

    #[test]
    fn test_note_to_freq() {
        assert!((note_to_freq("A4") - 440.0).abs() < 1e-5);
        assert!((note_to_freq("C4") - 261.625565).abs() < 1e-4);
    }
}
