# Interoperability Test Procedures

## Test Matrix

| Sender | macOS Version | Status | Notes |
|--------|---------------|--------|-------|
| iTunes | 12.x (macOS) | Pending | Primary test target |
| Music.app | macOS 11+ | Pending | Modern macOS |
| iOS | 14+ | Pending | iPhone/iPad |
| OwnTone | Latest | Pending | Open source sender |
| Roon | Latest | Pending | Audiophile software |

## Test Procedure

### 1. Discovery Test
1. Start receiver with known name
2. Open sender application
3. Verify receiver appears in device list
4. Verify icon/name display correctly

### 2. Basic Playback Test
1. Connect to receiver
2. Play audio file
3. Verify audio output
4. Verify no clicks/pops
5. Verify timing stability

### 3. Volume Test
1. During playback, adjust volume
2. Verify receiver responds
3. Test mute/unmute
4. Verify smooth transitions

### 4. Metadata Test
1. Play track with metadata
2. Verify title received
3. Verify artist received
4. Verify artwork received

### 5. Session Test
1. Start playback
2. Pause/resume
3. Switch tracks
4. Disconnect cleanly

### 6. Preemption Test
1. Connect sender A
2. Start playback
3. Connect sender B
4. Verify A disconnected
5. Verify B plays correctly

## Reporting Issues

Document any failures with:
- Sender version
- Receiver log output
- Packet captures if available
- Steps to reproduce
