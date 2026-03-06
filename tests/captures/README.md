# AirPlay 2 Test Captures

This directory contains packet captures for testing. To create new captures:

## Capturing from iOS/macOS

1. Use Wireshark to capture traffic between an iOS device and a known AirPlay receiver
2. Filter for the AirPlay port (usually 7000)
3. Export as hex dump using the format below

## Capture File Format

Plain text, one packet per line:
```
timestamp_us direction protocol hex_data
```

- `timestamp_us`: Microseconds from start of capture
- `direction`: `IN` (sender→receiver) or `OUT` (receiver→sender)
- `protocol`: `TCP` or `UDP`
- `hex_data`: Packet payload as hex string

Example:
```
0 IN TCP 4f5054494f4e53202a20525453502f312e30...
1500 OUT TCP 525453502f312e3020323030204f4b...
```

## Required Captures

- [ ] `info_request.hex` - GET /info exchange
- [ ] `pairing_exchange.hex` - Full pair-setup and pair-verify
- [ ] `setup_phase1.hex` - SETUP for timing/event
- [ ] `setup_phase2.hex` - SETUP for audio
- [ ] `audio_streaming.hex` - RTP audio packets (encrypted)
- [ ] `volume_metadata.hex` - SET_PARAMETER for volume/metadata

## Sanitizing Captures

Before committing captures, remove:
- Real IP addresses (replace with 192.168.1.x)
- MAC addresses (replace with AA:BB:CC:DD:EE:FF)
- Personal device names
