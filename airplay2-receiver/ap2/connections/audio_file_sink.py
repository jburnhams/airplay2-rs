"""
File-based audio sink for testing without audio hardware dependencies.
This module provides a drop-in replacement for PyAudio that writes to a file.
"""

import os
import struct
import time


class FileAudioSink:
    """A mock audio sink that writes raw PCM data to a file."""

    def __init__(
        self, filename="received_audio.raw", channels=2, rate=44100, sample_width=2
    ):
        self.filename = filename
        self.channels = channels
        self.rate = rate
        self.sample_width = sample_width
        self.file = None
        self.bytes_written = 0
        self.start_time = None

    def open(self):
        """Open the file for writing."""
        self.file = open(self.filename, "wb")
        self.start_time = time.time()
        print(
            f"[FileAudioSink] Opened {self.filename} for writing (rate={self.rate}, channels={self.channels}, width={self.sample_width})"
        )
        return self

    def write(self, data):
        """Write audio data to the file."""
        if self.file:
            self.file.write(data)
            self.file.flush() # Force flush
            self.bytes_written += len(data)
            # Log progress periodically
            with open("sink_debug.log", "a") as f:
                 f.write(f"Wrote {len(data)} bytes\n")
            if (
                self.bytes_written % (self.rate * self.channels * self.sample_width)
                == 0
            ):
                # ... same ...
                pass
        return len(data)

    def get_output_latency(self):
        """Return fake latency for compatibility."""
        return 0.0

    def close(self):
        """Close the file and optionally convert to WAV."""
        if self.file:
            self.file.close()
            print(
                f"[FileAudioSink] Closed {self.filename} - total {self.bytes_written} bytes"
            )

            # Also write a WAV file for easy playback
            wav_filename = self.filename.replace(".raw", ".wav")
            self._write_wav(wav_filename)
            self.file = None

    def _write_wav(self, wav_filename):
        """Convert the raw file to WAV format."""
        try:
            # Read raw data
            with open(self.filename, "rb") as f:
                raw_data = f.read()

            # Write WAV file
            with open(wav_filename, "wb") as wav:
                # WAV header
                num_samples = len(raw_data) // (self.sample_width * self.channels)
                data_size = num_samples * self.channels * self.sample_width

                wav.write(b"RIFF")
                wav.write(struct.pack("<I", 36 + data_size))  # File size - 8
                wav.write(b"WAVE")
                wav.write(b"fmt ")
                wav.write(struct.pack("<I", 16))  # Subchunk1 size (PCM)
                wav.write(struct.pack("<H", 1))  # Audio format (PCM)
                wav.write(struct.pack("<H", self.channels))
                wav.write(struct.pack("<I", self.rate))
                wav.write(
                    struct.pack("<I", self.rate * self.channels * self.sample_width)
                )  # Byte rate
                wav.write(
                    struct.pack("<H", self.channels * self.sample_width)
                )  # Block align
                wav.write(struct.pack("<H", self.sample_width * 8))  # Bits per sample
                wav.write(b"data")
                wav.write(struct.pack("<I", data_size))
                wav.write(raw_data)

            print(f"[FileAudioSink] Converted to WAV: {wav_filename}")
        except Exception as e:
            print(f"[FileAudioSink] Error writing WAV: {e}")


class MockPyAudio:
    """A mock PyAudio class that creates FileAudioSinks."""

    def __init__(self):
        self.streams = []

    def get_format_from_width(self, width):
        """Return format constant for sample width."""
        return width

    def open(self, format=2, channels=2, rate=44100, output=True, frames_per_buffer=4):
        """Open a file audio sink."""
        sink = FileAudioSink(
            filename=f"received_audio_{rate}_{channels}ch.raw",
            channels=channels,
            rate=rate,
            sample_width=format,
        )
        sink.open()
        self.streams.append(sink)
        return sink

    def get_default_output_device_info(self):
        """Return fake device info."""
        return {
            "defaultLowOutputLatency": 0.01,
            "defaultHighOutputLatency": 0.05,
        }

    def terminate(self):
        """Close all streams."""
        for stream in self.streams:
            if stream.file:
                stream.close()
        self.streams = []


# Global flag to enable file sink mode
USE_FILE_SINK = os.environ.get("AIRPLAY_FILE_SINK", "0") == "1"


def get_audio_backend():
    """Return the appropriate audio backend based on configuration."""
    if USE_FILE_SINK:
        print("[Audio] Using FileAudioSink (file output mode)")
        return MockPyAudio()
    else:
        try:
            import pyaudio

            print("[Audio] Using PyAudio (hardware output mode)")
            return pyaudio.PyAudio()
        except ImportError:
            print("[Audio] PyAudio not available, using FileAudioSink")
            return MockPyAudio()
