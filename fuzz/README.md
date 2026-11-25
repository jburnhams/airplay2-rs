# Differential fuzzing against C implementations

AirPlay has no spec we can trust, so these targets check our code against C
implementations that work with real devices. Each target feeds the same input
to our Rust code and to a C/C++ reference (see [`oracles/`](oracles/README.md))
and fails if they disagree.

| Target | Ours | Reference |
| --- | --- | --- |
| `plist_vs_libplist` | `protocol::plist::decode` | libplist 2.3.0 |
| `tlv8_vs_pair_ap` | `protocol::pairing::tlv::TlvDecoder` | pair_ap (OwnTone, Shairport Sync) |
| `raop_audio_vs_shairport` | `RaopStreamer::encode_frame` (AirPlay 1 audio) | Shairport Sync's packet decryption |
| `alac_vs_shairport` | our ALAC encoder settings and fmtp | Apple's ALAC decoder and Hammerton's, as Shairport Sync uses them |

## Running

Needs nightly Rust, `cargo install cargo-fuzz`, a C/C++ compiler and OpenSSL
headers (`apt install libssl-dev` / `brew install openssl`).

```sh
cd fuzz
cargo +nightly fuzz run -O plist_vs_libplist -- -max_total_time=300
```

Seed the plist target with real plists for better coverage:
`mkdir -p corpus/plist_vs_libplist && cp ../tests/fixtures/*.bplist corpus/plist_vs_libplist/`.

CI runs each target for 2 minutes on PRs that touch the code under test and
for 30 minutes weekly (`.github/workflows/fuzz.yml`).

## Accepted differences

Where we are deliberately stricter than the reference, the target lets that
case through, with a comment in the target:

- **plist:** we reject what libplist accepts for: the `null` marker, invalid
  UTF-8 / unpaired UTF-16 surrogates, duplicate dictionary keys, 16-byte
  integers that don't fit in 64 bits, offset/ref sizes other than 1/2/4/8, and
  sets. libplist rejects empty UTF-16 strings, which we accept. Strings are
  compared up to the first NUL because libplist returns C strings.
- **ALAC:** Hammerton's decoder mis-decodes packets whose intermediate values
  need 17 bits (a step of more than 32767 in L, R or L-R, or |L-R| > 32767);
  Apple's decoder handles them, so the Hammerton check is skipped for those.

## Findings (2026-11-25)

First runs found a divergence in every target within seconds. All fixed in the
same PR unless noted.

1. **AirPlay 1 audio was encrypted with the wrong cipher.** We used AES-CTR;
   Shairport Sync, OwnTone and AirPort Express use AES-128-CBC, restarted
   from the session IV for each packet, leaving the trailing partial block
   in the clear. Our own receiver already decrypted with CBC, so sender and
   receiver in this repo disagreed. Any real AirPlay 1 device would have
   played noise.
2. **plist: 1-, 2- and 4-byte integers were sign-extended.** Apple and
   plistlib write 44100 as the 2 bytes `AC 44`; we decoded it as -21436.
   Our encoder avoided this by always using wider ints, so round-trip tests
   passed.
3. **plist: looser than CoreFoundation/libplist** on where objects and the
   offset table may sit, date markers other than `0x33`, UIDs over 32 bits
   (used to wrap silently), and `null` (used to become empty data). Duplicate
   keys are now an error instead of last-wins.
4. **TLV8: any repeated type was concatenated.** HAP only joins consecutive
   fragments where every one but the last is 255 bytes. We now do the same and
   return the first item of a type, like pair_ap.
5. **Not ours:** Hammerton's ALAC decoder (Shairport Sync's default) breaks on
   packets needing 17-bit intermediates, see above. Real music rarely does
   this, but full-scale square waves or clipped audio can.

Not found by fuzzing, but seen while wiring up the ALAC target and still open:
with `prefer_hires_audio` and ALAC, `PcmStreamer` passes 24-bit PCM to an
encoder configured for 16-bit input and announces `bit_depth=24` in the fmtp.
