# Integration Tests

This directory contains end-to-end integration tests that verify the complete AirPlay 2 streaming pipeline.

## Overview

The integration tests:
1. Start the Python `airplay2-receiver` as a subprocess
2. Connect the Rust client and stream audio
3. Verify the received audio output

## Requirements

### System Dependencies

**macOS**:
```bash
brew install portaudio ffmpeg
```

**Ubuntu/Debian**:
```bash
sudo apt-get install -y \
    libavformat-dev \
    libavcodec-dev \
    libavdevice-dev \
    libavutil-dev \
    libswscale-dev \
    libswresample-dev \
    libavfilter-dev \
    portaudio19-dev
```

### Python Dependencies

```bash
cd airplay2-receiver
pip install -r requirements.txt
```

## Running Tests Locally

Integration tests are marked with `#[ignore]` to prevent them from running in normal `cargo test`.

**Run all integration tests**:
```bash
cargo test --test integration_tests -- --ignored --test-threads=1 --nocapture
```

**Run specific test**:
```bash
cargo test --test integration_tests test_pcm_streaming_end_to_end -- --ignored --nocapture
```

**Why `--test-threads=1`?** The Python receiver binds to port 7000, so only one test can run at a time.

**Why `--nocapture`?** Shows test progress and receiver output in real-time.

## CI/CD

Integration tests run automatically in GitHub Actions on:
- Push to `main`
- Pull requests
- Manual workflow dispatch

See `.github/workflows/integration.yml` for configuration.

## Test Structure

### `test_pcm_streaming_end_to_end`
- Streams 3 seconds of 440Hz PCM audio
- Verifies audio file is created and has valid data
- Checks RTP packets were received
- Validates sine wave quality (amplitude, non-zero samples)

### `test_alac_streaming_end_to_end`
- Same as PCM test but with ALAC compression
- Verifies lossless encoding/decoding

## Debugging Failed Tests

When tests fail, artifacts are saved:

**Locally**:
- `target/integration-test-*.log` - Test logs
- `airplay2-receiver/received_audio_44100_2ch.raw` - Raw audio
- `airplay2-receiver/rtp_packets.bin` - RTP packets

**CI**:
- Uploaded as GitHub Actions artifacts
- Retained for 7 days

### Analyzing Audio Output

```bash
# Check file size
ls -lh airplay2-receiver/received_audio_44100_2ch.raw

# Convert to WAV for playback
ffmpeg -f s16le -ar 44100 -ac 2 -i received_audio_44100_2ch.raw output.wav

# Visualize waveform
ffmpeg -i output.wav -filter_complex showwavespic=s=1024x240 waveform.png
```

### Analyzing RTP Packets

```bash
# Check packet count (rough estimate)
wc -c airplay2-receiver/rtp_packets.bin

# View first packet header (hex)
hexdump -C airplay2-receiver/rtp_packets.bin | head -20
```

## Troubleshooting

### "Python receiver failed to start"
- Check Python dependencies: `pip install -r airplay2-receiver/requirements.txt`
- Verify port 7000 is available: `lsof -i :7000`
- Check system audio dependencies are installed

### "No audio data received"
- Increase timeout in test (receiver might be slow to start)
- Check receiver logs for errors
- Verify network interface is correct (use `AIRPLAY_TEST_INTERFACE=en0`)

### "Audio amplitude too low"
- Audio might be silent - check source generation
- Verify receiver is decoding properly (check logs)

### macOS specific: "Address already in use"
- Disable system AirPlay receiver: System Settings → AirDrop & Handoff → AirPlay Receiver → Off
- Kill any existing Python receiver: `pkill -f ap2-receiver.py`

## Adding New Tests

1. Create new test function with `#[tokio::test]` and `#[ignore]`
2. Follow pattern: start receiver → run client → verify output
3. Use `init()` for logging setup
4. Always stop receiver in cleanup (use `?` or explicit error handling)
5. Add verification assertions

Example:
```rust
#[tokio::test]
#[ignore]
async fn test_new_feature() -> Result<(), Box<dyn std::error::Error>> {
    init();

    let receiver = PythonReceiver::start().await?;
    sleep(Duration::from_secs(2)).await;

    // Test code here

    let output = receiver.stop().await?;
    output.verify_audio_received()?;

    Ok(())
}
```

## Performance Notes

- Integration tests take ~10-15 seconds each
- Python receiver startup: ~3 seconds
- Audio streaming: ~3 seconds
- Cleanup and verification: ~2 seconds

Total runtime for both tests: ~30 seconds
