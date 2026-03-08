import numpy as np
from scipy.io import wavfile
import os

os.makedirs('audios/simd_test', exist_ok=True)
sample_rate = 44100

def generate_tone(duration_sec, frequency, filename, noise=False):
    t = np.linspace(0, duration_sec, int(sample_rate * duration_sec), endpoint=False)
    audio = 0.5 * np.sin(2 * np.pi * frequency * t)
    if noise:
        audio += 0.1 * np.random.normal(size=len(t))
    # Convert to 16-bit PCM
    audio_int16 = np.int16(audio * 32767)
    wavfile.write(f'audios/simd_test/{filename}', sample_rate, audio_int16)

# 1) 3 short syllables (0.5s)
generate_tone(0.5, 440.0, 'short_01.wav')
generate_tone(0.5, 523.25, 'short_02.wav')
generate_tone(0.5, 659.25, 'short_03.wav')

# 2) 3 medium samples (2.0s)
generate_tone(2.0, 440.0, 'medium_01.wav')
generate_tone(2.0, 523.25, 'medium_02.wav')
generate_tone(2.0, 659.25, 'medium_03.wav')

# 3) 3 long samples (5.0s)
generate_tone(5.0, 440.0, 'long_01.wav')
generate_tone(5.0, 523.25, 'long_02.wav')
generate_tone(5.0, 659.25, 'long_03.wav')

# 4) 1 extreme case (high pitch/noise - 2.0s)
generate_tone(2.0, 2000.0, 'extreme_high_noise.wav', noise=True)

paths = [
    'audios/simd_test/short_01.wav',
    'audios/simd_test/short_02.wav',
    'audios/simd_test/short_03.wav',
    'audios/simd_test/medium_01.wav',
    'audios/simd_test/medium_02.wav',
    'audios/simd_test/medium_03.wav',
    'audios/simd_test/long_01.wav',
    'audios/simd_test/long_02.wav',
    'audios/simd_test/long_03.wav',
    'audios/simd_test/extreme_high_noise.wav'
]

with open('samples.txt', 'w') as f:
    for p in paths:
        f.write(p + '\n')
