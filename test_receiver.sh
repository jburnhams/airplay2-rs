#!/bin/bash
# Start the AirPlay 2 receiver in file-sink mode for testing

cd "$(dirname "$0")/airplay2-receiver"

# Set environment variable to use file sink instead of pyaudio
export AIRPLAY_FILE_SINK=1

# Run the receiver
# -n specifies the network interface
# -m specifies the mDNS name
# --debug enables debug output

echo "Starting AirPlay 2 receiver (file-sink mode)..."
echo "Output will be written to received_audio_*.wav files"
echo ""

python3 ap2-receiver.py -n en0 -m "airplay2-rs-test" --debug
