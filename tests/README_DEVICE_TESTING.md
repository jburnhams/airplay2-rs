# Real Device Testing Procedures

## Supported Test Devices

- AirPort Express (1st/2nd generation)
- Apple TV (2nd/3rd generation)
- HomePod (AirPlay 1 mode)
- Third-party RAOP receivers (Shairport-sync)

## Test Environment Setup

1. **Network Configuration**
   - Test device and computer on same network
   - Multicast traffic enabled (for mDNS)
   - UDP ports 5000-7000 accessible

2. **Test Device Preparation**
   - Reset device to factory defaults
   - No password protection (for initial tests)
   - Connected to audio output (speakers/headphones)

## Manual Test Checklist

### Discovery Tests

- [ ] Device appears in mDNS scan within 5 seconds
- [ ] TXT records parsed correctly
- [ ] MAC address extracted from service name
- [ ] Capabilities match device specifications

### Connection Tests

- [ ] OPTIONS request succeeds
- [ ] Apple-Challenge verified (if required)
- [ ] ANNOUNCE with SDP accepted
- [ ] SETUP returns valid transport parameters
- [ ] RECORD starts without error

### Audio Streaming Tests

- [ ] Audio plays within 500ms of first packet
- [ ] No audible glitches with continuous stream
- [ ] Volume changes take effect
- [ ] Playback stops on TEARDOWN

### Metadata Tests

- [ ] Track title displays on device (if supported)
- [ ] Artist/album displays correctly
- [ ] Artwork displays (if device has screen)
- [ ] Progress bar updates

### Error Recovery Tests

- [ ] Reconnects after network interruption
- [ ] Handles device sleep/wake
- [ ] Recovers from packet loss (retransmission)

## Automated Device Tests

Run with actual device:

```bash
RAOP_TEST_DEVICE=192.168.1.50:5000 cargo test --test raop_device_tests
```

## Known Device Quirks

| Device | Issue | Workaround |
|--------|-------|------------|
| AirPort Express Gen1 | Slow OPTIONS response | Increase timeout to 5s |
| Some Shairport builds | No Apple-Challenge | Disable challenge verification |
| HomePod | Prefers AirPlay 2 | Force RAOP with config |
