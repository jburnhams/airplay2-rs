use std::f32::consts::PI;
use std::fs::File;
use std::io::Write;

use airplay2::audio::AudioFormat;
use airplay2::streaming::AudioSource;

/// Stereo test source: 440Hz Left, 880Hz Right
struct StereoSource {
    phase_l: f32,
    phase_r: f32,
    freq_l: f32,
    freq_r: f32,
    format: AudioFormat,
    samples_generated: u64,
    max_samples: u64,
}

impl StereoSource {
    fn new(duration_secs: u32) -> Self {
        let format = AudioFormat::CD_QUALITY; // 16-bit 44.1kHz stereo
        let max_samples = u64::from(duration_secs) * u64::from(format.sample_rate.as_u32());
        Self {
            phase_l: 0.0,
            phase_r: 0.0,
            freq_l: 440.0,
            freq_r: 880.0,
            format,
            samples_generated: 0,
            max_samples,
        }
    }
}

impl AudioSource for StereoSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let sample_rate = self.format.sample_rate.as_u32() as f32;

        if self.samples_generated >= self.max_samples {
            return Ok(0); // EOF
        }

        let mut bytes_written = 0;
        for chunk in buffer.chunks_exact_mut(4) {
            if self.samples_generated >= self.max_samples {
                break;
            }

            // Left Channel (440Hz)
            let sample_l = (self.phase_l * 2.0 * PI).sin();
            #[allow(clippy::cast_possible_truncation)]
            let value_l = (sample_l * i16::MAX as f32 * 0.5) as i16;
            let bytes_l = value_l.to_be_bytes();

            // Right Channel (880Hz)
            let sample_r = (self.phase_r * 2.0 * PI).sin();
            #[allow(clippy::cast_possible_truncation)]
            let value_r = (sample_r * i16::MAX as f32 * 0.5) as i16;
            let bytes_r = value_r.to_be_bytes();

            chunk[0] = bytes_l[0];
            chunk[1] = bytes_l[1];
            chunk[2] = bytes_r[0];
            chunk[3] = bytes_r[1];

            self.phase_l += self.freq_l / sample_rate;
            if self.phase_l > 1.0 {
                self.phase_l -= 1.0;
            }

            self.phase_r += self.freq_r / sample_rate;
            if self.phase_r > 1.0 {
                self.phase_r -= 1.0;
            }

            self.samples_generated += 1;
            bytes_written += 4;
        }

        Ok(bytes_written)
    }
}

fn main() -> std::io::Result<()> {
    let mut source = StereoSource::new(3);
    let mut file = File::create("test_stereo.raw")?;
    let mut buffer = [0u8; 4096];

    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
    }
    println!("Generated test_stereo.raw");
    Ok(())
}
