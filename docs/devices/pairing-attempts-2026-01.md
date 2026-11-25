# AirPlay 2 Implementation Status & Investigation

**Date:** January 24, 2026
**Status:** Handshake OK -> Pairing Initiated -> Authentication Failed (Wrong PIN/Secret)

## Overview
This document summarizes the attempts to implement a functioning AirPlay 2 client using `airplay2-rs`. We successfully established communication, completed MFi `auth-setup`, and **successfully initiated SRP Pairing** (bypassing the previous 403 Forbidden error). However, the pairing fails at the verification step (M3 -> M4), indicating that the devices reject the fixed PIN "3939".

## Device Matrix

| Device Name | Model | Status | Behavior |
|-------------|-------|--------|----------|
| **One** | Sonos One | **Auth Failed** | Accepts `auth-setup` (200). Accepts `pair-setup` M1 (200). Rejects M3 Proof (Authentication Error). |
| **HW-Q990C** | Samsung Soundbar | **Auth Failed** | Rejects `auth-setup` (400). Accepts `pair-setup` M1 (200). Rejects M3 Proof (Authentication Error). |
| **AirPort10,115** | AirPort Express | **Failed** | Rejects `pair-setup` M1 (403 Forbidden). Might require exact method/flags combo or HomeKit. |
| **Mac16,1** | UxPlay | **Failed** | Rejects `OPTIONS` (403). Likely requires Legacy AirPlay or has a custom PIN. |

## Key Discoveries

### 1. Protocol Headers are Critical
*   **Discovery:** The missing link for `POST /pair-setup` was the `X-Apple-HKP` header.
*   **Fix:** Added `X-Apple-HKP: 4` (Transient Mode) to pairing requests.
*   **Result:** Sonos and Samsung devices stopped sending `403 Forbidden` and started sending `200 OK` with SRP Salt/Key!

### 2. "Transient" Pairing is SRP
*   **Discovery:** Method 0 (Transient) in AirPlay 2 does *not* use Curve25519 directly (as previously thought). It uses **SRP-6a** with a fixed PIN (usually "3939") and `Flags=0x10`.
*   **Status:** We implemented this flow correctly. The devices respond with SRP parameters (N=3072 bit).

### 3. Authentication Error (Backoff)
*   **Issue:** After sending the Client Proof (M3), devices respond with `M4` containing `Error: Authentication (2)` or `Error: Backoff (3)`.
*   **Meaning:** The device successfully computed the proof but it didn't match ours. This almost certainly means **the PIN "3939" is incorrect** for these specific devices (Sonos/Samsung).
*   **Implication:** These devices likely do not support "Open/Transient" pairing in their current configuration and require a real, random PIN (HomeKit style) or a different fixed PIN.

## Recommendations

1.  **Try HomeKit Pairing**: Implementing the full HomeKit Accessory Protocol (HAP) pairing flow (on port 5000+) is likely the only way to generate a valid pairing identity for these commercial devices.
2.  **Legacy AirPlay**: Fallback to RAOP (AirPlay 1) might work for Sonos/UxPlay if AirPlay 2 pairing proves too difficult.
3.  **User PIN Entry**: If the device logic can be triggered to display a PIN (via Method 1?), we could prompt the user. But current attempts to trigger this (Method 1 via RTSP) also failed or behaved identically.
