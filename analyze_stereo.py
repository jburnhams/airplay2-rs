import numpy as np
import sys


def analyze_stereo(filename, sample_rate=44100):
    print(f"Analyzing {filename}...")

    try:
        with open(filename, "rb") as f:
            data = f.read()
    except FileNotFoundError:
        print(f"Error: File {filename} not found")
        return False

    # Convert raw bytes to numpy array (16-bit Big Endian)
    dt = np.dtype(np.int16)
    dt = dt.newbyteorder(">")
    audio = np.frombuffer(data, dtype=dt)

    # Reshape to (num_samples, 2)
    try:
        audio = audio.reshape(-1, 2)
    except ValueError:
        print("Error: Data size not aligned to stereo frames")
        return False

    left = audio[:, 0]
    right = audio[:, 1]

    if len(left) == 0:
        print("Error: File is empty")
        return False

    print(f"Read {len(left)} samples")

    # Analyze frequency using FFT
    start = len(left) // 4
    end = start + 44100  # 1 second window
    if end > len(left):
        end = len(left)

    chunk_l = left[start:end]
    chunk_r = right[start:end]

    def get_peak_freq(chunk, name):
        fft = np.fft.fft(chunk)
        freqs = np.fft.fftfreq(len(chunk), 1 / sample_rate)
        magnitude = np.abs(fft)
        peak_idx = np.argmax(magnitude[: len(chunk) // 2])
        peak_freq = freqs[peak_idx]
        print(f"{name} Peak frequency: {peak_freq:.2f} Hz")
        return peak_freq

    freq_l = get_peak_freq(chunk_l, "Left")
    freq_r = get_peak_freq(chunk_r, "Right")

    success = True

    # Check Left (440Hz)
    if abs(freq_l - 440.0) < 5.0:
        print("LEFT CHANNEL: PASS (440Hz)")
    else:
        print(f"LEFT CHANNEL: FAIL (Expected 440Hz, got {freq_l:.2f} Hz)")
        success = False

    # Check Right (880Hz)
    if abs(freq_r - 880.0) < 5.0:
        print("RIGHT CHANNEL: PASS (880Hz)")
    else:
        print(f"RIGHT CHANNEL: FAIL (Expected 880Hz, got {freq_r:.2f} Hz)")
        success = False

    return success


if __name__ == "__main__":
    filename = "airplay2-receiver/received_audio_44100_2ch.raw"
    if len(sys.argv) > 1:
        filename = sys.argv[1]

    if analyze_stereo(filename):
        sys.exit(0)
    else:
        sys.exit(1)
