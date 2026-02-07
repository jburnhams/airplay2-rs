```markdown
# AirPlay 2 → HomePod Gen 1: Exhaustive AAC Playback Flow

This document describes, at a low level, what an AirPlay 2 **client (sender)** must do to stream an **AAC audio file** to a **HomePod gen 1**.

---

## 0. Preconditions and Concepts

- HomePod advertises:
  - `_airplay._tcp` (AirPlay 2)
  - `_raop._tcp` (RAOP / AirPlay 1)
  - `_hap._tcp` (HomeKit Accessory Protocol)[web:49]
- AirPlay 2 uses:
  - RTSP with HTTP‑like requests over TCP[web:88]
  - HomeKit pairing (HAP) for trust and control‑channel keys[web:20]
  - ChaCha20‑Poly1305 for control channel encryption[web:20]
  - FairPlay + AES for media keys (for protected content)[web:18][web:30]
- HomePod must already be in the user’s HomeKit home in most realistic scenarios.

---

## 1. Discovery

1. Start **Bonjour/mDNS browse** for `_airplay._tcp`:
   - Query: `_airplay._tcp.local.`
   - Collect SRV + TXT records for each instance.

2. For each `_airplay._tcp` service:
   - Resolve hostname and port.
   - Parse TXT record keys (partial list)[web:49][web:43]:
     - `deviceid` – device identifier (MAC‑like).
     - `features` – 64‑bit feature bitmask (string or hex).
     - `rsf` – required sender features bitmask.
     - `model` – e.g. `AudioAccessory1,1` for HomePod gen 1.
     - `pk` – AirPlay public key.
     - `pi` / `psi` – pairing identifiers (AirPlay and system pairing).
     - `hkid` / `hmid` / `hgid` – HomeKit IDs (home, household, group).
     - Optional: `srcvers`, `flags`, etc.

3. Filter candidates:
   - `features` must include bits for:
     - AirPlay 2 buffered / PTP (e.g. buffered audio, PTP sync)[web:40][web:90].
     - HomeKit pairing / access control (e.g. `SupportsHKPairingAndAccessControl`)[web:40].
   - Ensure your sender meets `rsf` (required sender features).

4. Pick the target HomePod based on:
   - Name/hostname.
   - Feature set.
   - User choice (UI).

---

## 2. HomeKit Pairing (HAP)

### 2.1 When full HomeKit pairing is needed

If the HomePod is **not yet paired** to any HomeKit controller (rare in practice for a HomePod):

1. Discover `_hap._tcp` service for the same device (matching `deviceid`)[web:47][web:49].
2. Initiate HAP pairing (controller role)[web:20]:
   - `POST /pair-pin-start`  
     - Instructs accessory to show/display PIN.
   - `POST /pair-setup` (SRP‑based PAKE)[web:19]:
     - Exchange SRP parameters (username, salt, A, B).
     - Verify password/PIN proofs (M3/M4).
     - Derive SRP shared secret.
   - `POST /pair-verify`[web:19][web:20]:
     - Use Curve25519 ephemeral keys + Ed25519 signatures to authenticate long‑term keys.
     - Derive a session key (`SessionKey`).

3. Store:
   - Accessory ID and Ed25519 public key.
   - Controller ID and keys.
   - Association of this HomePod to your HomeKit “home”.

Most AirPlay clients will not implement this directly; the OS (iOS/macOS) already handles it.

### 2.2 Transient or system pairing

If you only need a **short‑lived pairing** (transient pairing), or you are a secondary sender:

1. Run **HomeKit “transient pairing”** against the AirPlay service[web:20]:
   - Use `POST /pair-setup` only (no full pairing record kept).
   - Use fixed internal PIN `3939` (per HAP notes)[web:20][web:19].
   - Derive `EncryptionKey` directly from SRP shared secret.

2. For a device already known to HomeKit:
   - Use `/pair-verify` with existing Ed25519 credentials to derive `EncryptionKey` from `SessionKey`[web:20].

Result: you get a per‑session **`EncryptionKey`** used for control channel encryption.

---

## 3. Control Channel Establishment

### 3.1 TCP connection and RTSP basics

1. Open a **TCP connection** to the HomePod’s `_airplay._tcp` host:port.

2. All subsequent AirPlay 2 signaling uses **Apple RTSP**:
   - RTSP request line: `METHOD path RTSP/1.0` (e.g. `GET /info RTSP/1.0`)[web:88].
   - Headers:
     - `X-Apple-ProtocolVersion: 1`
     - `Content-Length: N` (if body)
     - `Content-Type: application/x-apple-binary-plist` (when plist body present)
     - `CSeq: N` – sequence number, incremented per request.
     - Optional: `DACP-ID`, `Active-Remote` (for DACP remote control)[web:88].
   - Response echoes the same `CSeq`.

### 3.2 Derive ChaCha20‑Poly1305 keys from EncryptionKey

From the HomeKit‑based pairing spec for AirPlay[web:20]:

- Input: `EncryptionKey` from transient or normal pairing.
- Use HKDF‑SHA512 to derive two 32‑byte keys:

  - Client → accessory (write) key:
    - salt = `"Control-Salt"`
    - info = `"Control-Write-Encryption-Key"`
  - Accessory → client (read) key:
    - salt = `"Control-Salt"`
    - info = `"Control-Read-Encryption-Key"`

- Cipher: **ChaCha20‑Poly1305** AEAD.
- Nonce: 96‑bit, with:
  - upper 32 bits = 0
  - lower 64 bits = counter starting at 0 per direction[web:20].

### 3.3 Encrypted control frame format

All further RTSP bodies (including `SETUP`/`ANNOUNCE` plists) are typically wrapped as[web:20]:

- `length (2 bytes LE)` – number of ciphertext bytes.
- `ciphertext` – ChaCha20 output.
- `tag (16 bytes)` – Poly1305 tag.

The sender:

1. Serializes a plist body.
2. Encrypts with ChaCha20‑Poly1305 using the write key + current nonce.
3. Prepends 2‑byte length and appends 16‑byte tag.
4. Sends RTSP headers and then this encrypted frame.

The receiver:

1. Reads 2‑byte length.
2. Reads ciphertext + tag.
3. Decrypts with read key + its own nonce.

---

## 4. Optional: MFi Authentication and FairPlay

Depending on `features` and content:

### 4.1 MFi authentication (`/auth-setup`)

If HomePod advertises `SupportsUnifiedPairSetupAndMFi` & relevant auth bits[web:40][web:18]:

1. Client sends:

   ```http
   POST /auth-setup RTSP/1.0
   Content-Length: N
   Content-Type: application/octet-stream
   CSeq: X

   [challenge blob]
```

2. HomePod’s MFi chip signs/responds.
3. Client verifies response against MFi CA certificates[web:18].

This step proves the receiver is an Apple‑licensed device.

### 4.2 FairPlay authentication (`/fp-setup`)

For **protected streams** (e.g. Apple Music, DRM’d HLS):

1. Client sends:

```http
POST /fp-setup RTSP/1.0
Content-Length: N
Content-Type: application/octet-stream
CSeq: Y

[FairPlay handshake data]
```

2. HomePod and client complete **FairPlay v3** handshake:
    - Establish “FP session” keys.
    - Allow secure transfer of AES content keys later[web:18][web:30].
3. The AES key used to encrypt AAC frames is derived/transported via FairPlay, not HAP.

For unprotected AAC files, a minimal implementation may skip FairPlay, subject to how strictly the sender OS enforces it.

---

## 5. RTSP Signaling Sequence

### 5.1 `/info` – receiver capabilities

1. Initial **`GET /info`** may be:
    - With plist body: `Content-Type: application/x-apple-binary-plist`.
        - Request body (in clear or inside encrypted frame) like[web:88]:

```python
{'qualifier': ['txtAirPlay']}
```

        - This asks the receiver to return its `_airplay._tcp` TXT record as a binary plist.
    - Without body: plain `GET /info`, receiver replies with binary plist describing:
        - `initialVolume`
        - other capabilities (implementation‑defined)[web:88].
2. Client uses `/info` response to refine capabilities beyond DNS TXT (e.g. additional flags not in TXT).

### 5.2 `/audioMode` (optional)

Before `SETUP`, sender may configure audio mode[web:88]:

- `POST /audioMode` with plist body (encrypted):
    - e.g. set to audio‑only, specify spoken audio vs long‑form, etc.
- Response: confirms chosen audio mode.

This is optional; some clients skip it.

---

## 6. SETUP – Channels and Plists

AirPlay 2 uses **two SETUP phases** for audio[web:88]:

1. **SETUP \#1 – info + event (and timing)**
2. **SETUP \#2 – control + data (audio stream)**

Both bodies are **binary plists** wrapped inside the encrypted control channel.

### 6.1 SETUP \#1 – info and event channels

Purpose: define generic sender info, encryption parameters, timing configuration and open the event channel[web:88].

#### 6.1.1 Request format

```http
SETUP rtsp://<host>/<session-id> RTSP/1.0
Content-Length: N
Content-Type: application/x-apple-binary-plist
CSeq: 1
DACP-ID: <id>
Active-Remote: <token>
User-Agent: AirPlay/<build>

[encrypted binary plist]
```


#### 6.1.2 Body (plist) – typical keys

The plist contains keys such as[web:88]:

- `timingProtocol` – e.g. `PTP` or `NTP`.
- `timingPeerInfo`, `timingPeerList` – relevant if `PTP` is used.
- Encryption info:
    - `ekey` – encrypted (or raw) key for event/control/time channels.
    - `eiv` – IV for those channels.
    - `et` – encryption type enum (e.g. which cipher is applied).
- Generic info about sender device (not fully standardized; client‑specific).

The exact naming may vary across OS versions; reverse‑engineered servers log them as such.

#### 6.1.3 Response

HomePod replies with a plist containing:

- `eventPort` – port for event channel (TCP)[web:88].
- `timingPort` – port for timing (if using NTP; omitted/unused for PTP)[web:88].
- Optional: updated encryption info.

Sender must:

1. Open TCP connection to `eventPort`.
2. If `timingPort` provided (NTP mode), open timing channel; for PTP, use PTP stack instead.

Event channel must be established for RTSP to continue[web:88].

### 6.2 SETUP \#2 – control and data (audio)

Purpose: configure audio transport and open control/data channels for media[web:88][web:90].

#### 6.2.1 Request format

```http
SETUP rtsp://<host>/<session-id> RTSP/1.0
Content-Length: M
Content-Type: application/x-apple-binary-plist
CSeq: 2
DACP-ID: <id>
Active-Remote: <token>
User-Agent: AirPlay/<build>

[encrypted binary plist]
```


#### 6.2.2 Body (plist) – required keys (audio)

Per AirPlay 2 internals[web:88][web:90]:

- `type` – stream type:
    - `96` – general audio, real time.
    - `103` – general audio, buffered.
    - For HomePod multi‑room you typically use **103** (buffered)[web:88].
- `ct` – compression type:
    - `1` – LPCM
    - `2` – ALAC
    - `4` – **AAC**
    - `8` – AAC‑ELD
    - `32` – OPUS[web:88].
- `audioFormat` – bitmask encoding sample rate, depth, codec[web:90]:
    - For AAC:
        - `0x400000` – AAC‑LC/44100/2 (bit 22).
        - `0x800000` – AAC‑LC/48000/2 (bit 23).
        - and other AAC‑ELD values (bits 24–27, 31–32)[web:90].
- `spf` – frames per packet (AAC frames per RTP packet)[web:88].
- `latencyMs` / `audioLatencies` – desired buffering latency (ms)[web:80].
- `shk` – shared encryption key for audio:
    - Usually a per‑stream AES key (wrapped or referenced via FairPlay)[web:88].
- Additional encryption parameters:
    - e.g. `aiv` (audio IV), `at` (audio encryption type) – naming is implementation‑specific, but patterns are analogous to `ekey`/`eiv`/`et`[web:88].
- Control/transport hints:
    - `controlPort` (sender port) → used by receiver for RTCP.
    - `dataPort` (sender port) → used by receiver for RTP (if UDP).
    - Or flags indicating TCP interleaving for data in the main RTSP connection.

Conceptually, for a **single buffered AAC LC 44.1kHz stereo** stream:

```plist
{
  "type": 103,                // Buffered audio
  "ct": 4,                    // AAC
  "audioFormat": 0x400000,    // AAC-LC/44100/2[^1]
  "spf": 1024,                // AAC frames per packet (example)
  "audioLatencies": [ ... ],  // or latencyMs, etc.
  "shk": <AES key or wrapped key>,
  "aiv": <16-byte IV>,
  "at": 1,                    // encryption type (e.g. AES-CTR/AES-CBC) – impl-defined
  "controlPort": <your-rtcp-port-or-0>,
  "dataPort": <your-rtp-port-or-0>
}
```

All inside the encrypted control frame.

#### 6.2.3 Response

HomePod replies with plist that:

- Confirms or overrides:
    - `controlPort` – remote RTCP port the sender should send to.
    - `dataPort` – remote RTP port the sender should send to[web:88].
    - `audioFormat` – may echo what was accepted.
    - `spf`, `audioLatencies`, etc.
- May provide:
    - Final `shk`, `aiv`, `at` values to actually use.

Sender must:

1. Use returned **`dataPort`** for sending AAC RTP (or buffered TCP) packets.
2. Use **`controlPort`** for RTCP feedback.
3. Apply returned AES key + IV for audio payload encryption if required.

---

## 7. RECORD, FLUSH, and Data Flow

### 7.1 RECORD – start streaming

1. After successful SETUP, sender issues:

```http
RECORD rtsp://<host>/<session-id> RTSP/1.0
CSeq: 3
User-Agent: AirPlay/<build>

[optional encrypted plist]
```

2. HomePod responds OK; at this point the sender may begin sending RTP or buffered audio.

### 7.2 FLUSH – start of a new segment

1. Immediately before beginning audio (or when seeking), send:

```http
FLUSH rtsp://<host>/<session-id> RTSP/1.0
CSeq: 4
RTP-Info: seq=<first_seq>;rtptime=<first_timestamp>
```

2. `RTP-Info` specifies:
    - `seq` – first RTP sequence number that will be used.
    - `rtptime` – first RTP timestamp[web:88].

HomePod uses this to align its internal clock and buffers.

### 7.3 RTP and RTCP

1. **RTP (AAC payloads)**:
    - If UDP:
        - Send to HomePod `dataPort` with negotiated `type` (payload type 96/103 semantics).
    - If buffered/TCP:
        - Audio may be carried via interleaved TCP or an AP2 buffered channel; details depend on implementation.
2. **AAC framing**:
    - Each packet contains `spf` AAC frames.
    - AAC AudioSpecificConfig (if needed) has already been effectively communicated at `SETUP` level via `audioFormat` and codec metadata.
3. **Encryption**:
    - For protected content:
        - AAC frame payload is encrypted with AES; key is `shk` and IV `aiv` (or equivalent), negotiated via FairPlay and SETUP plist[web:88][web:18].
    - For unprotected content:
        - Key may be omitted or set to “no encryption”, depending on what receiver supports.
4. **RTCP**:
    - Sent to `controlPort` (UDP or TCP) and used for:
        - Sender reports (SR).
        - Receiver reports (RR).
        - Packet loss, jitter feedback, etc.[web:88][web:90].

---

## 8. Playback Control and Session Management

### 8.1 SET_PARAMETER / GET_PARAMETER

Control volume and metadata via RTSP:

- Volume:

```http
SET_PARAMETER rtsp://<host>/<session-id> RTSP/1.0
Content-Type: text/parameters
Content-Length: ...

volume: -10.0
```

    - Volume in dB, range typically −144 to 0[web:88].
- Progress / position:

```http
SET_PARAMETER ... 
Content-Type: text/parameters

progress: start/current/end
```

- Artwork and rich metadata:
    - `Content-Type: image/jpeg` – album art.
    - `Content-Type: application/x-dmap-tagged` – DAAP now playing info[web:88].


### 8.2 Feedback and heartbeat

- `POST /feedback`:
    - Periodically issued to confirm receiver liveness[web:88].
    - Typically no or minimal body.


### 8.3 TEARDOWN

To stop playback:

1. Send:

```http
TEARDOWN rtsp://<host>/<session-id> RTSP/1.0
CSeq: N
Content-Type: application/x-apple-binary-plist
Content-Length: ...

[encrypted binary plist]
```

2. Body plist usually lists active streams (for pause) or is empty (disconnect)[web:88].
3. Close RTP/RTCP/event/timing sockets after acknowledgment.

---

## 9. Summary Checklist for an AAC Client

1. **Discover** HomePod via `_airplay._tcp`; parse TXT record and choose target[web:49][web:43].
2. **Ensure pairing**:
    - Use HAP `/pair-setup` + `/pair-verify` (normal) or transient pairing[web:20].
    - Derive `EncryptionKey` and then ChaCha20‑Poly1305 control keys via HKDF‑SHA512[web:20].
3. **Open RTSP TCP** to HomePod.
4. **Optionally** run `/auth-setup` (MFi) and `/fp-setup` (FairPlay) as required by features/content[web:18].
5. **GET /info`** to fetch extended AirPlay capabilities[web:88].
6. **(Optional) `/audioMode`** to set audio mode[web:88].
7. **SETUP \#1**:
    - Send encrypted plist with `timingProtocol`, `ekey`, `eiv`, `et`, etc.[web:88].
    - Receive and open `eventPort` and optionally `timingPort`[web:88].
8. **SETUP \#2**:
    - Send encrypted plist with:
        - `type = 103` (buffered audio).
        - `ct = 4` (AAC).
        - `audioFormat = 0x400000` or `0x800000` for AAC‑LC stereo[web:90].
        - `spf` (AAC frames per packet).
        - `shk` / `aiv` / encryption type.
        - `controlPort` / `dataPort` (sender side).
    - Receive confirmed `controlPort`/`dataPort` and possibly adjusted format[web:88][web:90].
9. **RECORD + FLUSH**:
    - Issue `RECORD`.
    - Issue `FLUSH` with `RTP-Info: seq=...;rtptime=...`[web:88].
10. **Send AAC RTP**:
    - Encrypt payload with AES if protected; use `shk`/`aiv`.
    - Maintain buffer and timestamps according to negotiated latency[web:88][web:90].
11. **Control** via `SET_PARAMETER` (volume, progress, metadata)[web:88].
12. **Heartbeat** with `/feedback` and **stop** with `TEARDOWN`[web:88].

This is sufficient to interoperate with a HomePod gen 1 as an AirPlay 2 AAC sender, assuming HomeKit pairing and OS‑level constraints allow the connection.
