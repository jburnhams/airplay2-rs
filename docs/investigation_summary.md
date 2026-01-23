# AirPlay 2 Implementation Status & Investigation

**Date:** January 23, 2026
**Status:** Partial Success (Handshake/Auth-Setup) -> Blocked (Pairing Forbidden)

## Overview
This document summarizes the attempts to implement a functioning AirPlay 2 client using `airplay2-rs`. We successfully established communication, performed device identification, and completed the MFi `auth-setup` handshake. However, we are currently blocked by **403 Forbidden** responses during the pairing phase on all tested devices.

## Device Matrix

| Device Name | Model | Type | Status | Behavior |
|-------------|-------|------|--------|----------|
| **One** | Sonos One | Speaker | **Blocked** | Accepts TCP, `GET /info`, `POST /auth-setup` (OK). Rejects `POST /pair-setup` (403). |
| **AirPort10,115** | AirPort Express | Bridge | **Blocked** | Accepts TCP, `GET /info`, `POST /auth-setup` (OK). Rejects `POST /pair-setup` (403). |
| **HW-Q990C** | Samsung Soundbar | Speaker | **Failed** | Rejects `POST /auth-setup` (400 Bad Request). Likely does not support MFi auth flow or requires Legacy AirPlay. |
| **Mac16,1** | UxPlay (Linux) | Software | **Failed** | Rejects initial `OPTIONS` request (403 Forbidden). Likely requires different `User-Agent` or Legacy RAOP flow. |
| **QCQS9X** | Unknown | - | **Failed** | Rejects initial `OPTIONS` (403). |

## Order of Operations (Discovered)

Through iterative testing, we established the following required sequence for modern AirPlay 2 devices:

1.  **Network Connection**:
    *   **Issue:** Link-local IPv6 (`fe80::...`) connections fail on macOS without specific Scope ID (`%en0`).
    *   **Fix:** Prioritize IPv4 addresses during discovery. Connection is established successfully on port 7000.

2.  **RTSP Handshake (`OPTIONS`)**:
    *   **Requirement:** Devices strictly enforce `User-Agent` and identification headers.
    *   **Fix:**
        *   `User-Agent: AirPlay/540.31` (or `iTunes/12.8`)
        *   `Active-Remote: 4294967295`
        *   `DACP-ID: <Random 64-bit Hex>`
    *   **Result:** `RTSP/1.0 200 OK` (on Sonos/AirPort).

3.  **Identification (`GET /info`)**:
    *   **Result:** Returns binary plist containing device capabilities (`features`, `statusFlags`, `pi`, `deviceID`).
    *   **Status:** **Working**.

4.  **MFi Authentication (`POST /auth-setup`)**:
    *   **Requirement:** Required by AirPlay 2 devices to prove the client is an Apple device (or pretending to be).
    *   **Implementation:** Curve25519 key exchange.
    *   **Result:** **Success**. Devices respond with their public key and signature (verified by length check).

5.  **Pairing (`POST /pair-setup`)**:
    *   **Goal:** Establish a session for streaming.
    *   **Attempt 1 (Transient - Method 0):** Used for "Anyone can stream" scenarios.
        *   Result: `403 Forbidden`.
    *   **Attempt 2 (Standard - Method 1):** Used for PIN pairing.
        *   Result: `403 Forbidden`.
    *   **Diagnosis:** The devices are refusing to initiate pairing via RTSP. This strongly suggests they require **HomeKit (HAP) Pairing** or have restricted access policies (e.g., "Only owners", "Only on this Home").

6.  **Streaming Setup (`SETUP`)**:
    *   **Attempt:** Skip pairing and try `SETUP` directly after `auth-setup`.
    *   **Result:** `455 Method Not Valid In This State`.
    *   **Meaning:** Pairing is mandatory.

## Technical Findings & Blockers

### 1. The "403 Forbidden" Wall
The primary blocker is the HTTP 403 response to `POST /pair-setup`.
*   **Cause:** Modern commercial AirPlay 2 devices (Sonos, etc.) often disable "Legacy" RTSP pairing in favor of HomeKit pairing.
*   **Implication:** To pair with these devices, a client likely needs to implement the HomeKit Accessory Protocol (HAP) to establish the initial trust relationship and exchange long-term keys. Once paired via HomeKit, the client would use `POST /pair-verify` on port 7000.

### 2. Header Sensitivity
Devices immediately sever the connection (RST) or return 403 if specific headers are missing from *any* request, specifically:
*   `Active-Remote`
*   `DACP-ID`
*   `User-Agent`

### 3. UxPlay Specifics
UxPlay behaves differently than Sonos. It rejects the initial `OPTIONS` request. This is likely because it expects a Legacy AirPlay 1 (RAOP) handshake (RSA signature injection) rather than the AirPlay 2 `auth-setup` flow, or it requires a PIN that triggers a different flow not yet handled.

## Recommendations for Next Steps

1.  **Investigate Legacy AirPlay (RAOP)**:
    *   Many of these devices (Sonos One, UxPlay) are backwards compatible with AirPlay 1 (RAOP).
    *   Legacy AirPlay uses a different pairing mechanism (`POST /fp-setup` or direct `ANNOUNCE` with RSA signatures).
    *   This might bypass the strict HomeKit pairing requirements of AirPlay 2.

2.  **HomeKit Pairing**:
    *   Implementing a full HomeKit Controller to pair with the accessories is the "correct" way for AirPlay 2, but represents a significant engineering effort outside the current scope of `airplay2-rs`.

3.  **Target "Open" Devices**:
    *   If possible, configure a target device (like UxPlay) to allow "Anyone" to stream without a PIN or password. This might enable the Transient Pairing flow to succeed.
