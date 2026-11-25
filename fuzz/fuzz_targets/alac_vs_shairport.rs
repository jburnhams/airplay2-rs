//! Differential: our ALAC encoding vs the two ALAC decoders Shairport Sync
//! ships (Apple's reference decoder and David Hammerton's).
//!
//! Encodes 352-frame 16-bit stereo packets with the same encoder settings as
//! `PcmStreamer::use_alac` (src/streaming/pcm.rs), configures the decoders from
//! the fmtp line we announce (src/connection/manager.rs) parsed the way
//! Shairport Sync parses it, and requires a bit-exact round trip. ALAC is
//! lossless, so any difference is a bug on one side.
#![no_main]

use airplay2::protocol::sdp::raop::AlacParameters;
use airplay2_fuzz::{apple_alac_decode, hammerton_alac_decode_packet};
use alac_encoder::{AlacEncoder, FormatDescription};
use libfuzzer_sys::fuzz_target;

/// The fmtp we announce for 16-bit 44.1 kHz ALAC (connection/manager.rs).
const FMTP: &str = "96 352 0 16 40 10 14 2 255 0 0 44100";
const FRAMES: usize = 352;
const PACKET_BYTES: usize = FRAMES * 4;

fn fmtp_fields() -> [i32; 12] {
    // Shairport Sync reads the fmtp as 12 integers (rtsp.c). Check our own
    // parser agrees with that reading first.
    let p = AlacParameters::parse(FMTP).expect("our fmtp must parse");
    assert_eq!(p.frames_per_packet, 352);
    let mut f = [0i32; 12];
    for (i, s) in FMTP.split_whitespace().enumerate() {
        f[i] = s.parse().unwrap();
    }
    f
}

/// True if the packet needs more than 16 bits somewhere in ALAC's
/// intermediate values: a step of more than `i16::MAX` between samples in L, R
/// or the stereo difference L-R, or L-R itself out of range.
fn has_17_bit_step(pcm: &[u8]) -> bool {
    let s: Vec<i32> = pcm
        .chunks_exact(2)
        .map(|b| i32::from(i16::from_le_bytes([b[0], b[1]])))
        .collect();
    let left: Vec<i32> = s.iter().step_by(2).copied().collect();
    let right: Vec<i32> = s.iter().skip(1).step_by(2).copied().collect();
    let side: Vec<i32> = left.iter().zip(&right).map(|(l, r)| l - r).collect();
    let max = i32::from(i16::MAX);
    side.iter().any(|d| d.abs() > max)
        || [&left, &right, &side]
            .iter()
            .any(|ch| ch.windows(2).any(|w| (w[1] - w[0]).abs() > max))
}

fuzz_target!(|data: &[u8]| {
    let fmtp = fmtp_fields();
    let mut encoder = AlacEncoder::new(&FormatDescription::alac(44100.0, FRAMES as u32, 2));
    let input_format = FormatDescription::pcm::<i16>(44100.0, 2);
    let mut out = vec![0u8; 4096];
    for chunk in data.chunks(PACKET_BYTES).take(6) {
        let mut pcm = chunk.to_vec();
        pcm.resize(PACKET_BYTES, 0); // PcmStreamer zero-pads short packets too
        let n = encoder.encode(&input_format, &pcm, &mut out);
        let packet = &out[..n.min(out.len())];
        assert_eq!(
            apple_alac_decode(&fmtp, packet),
            pcm,
            "Apple decoder disagrees"
        );
        // Hammerton's decoder mis-decodes packets whose intermediate values
        // need 17 bits (big sample steps or a big L-R difference); Apple's
        // decoder gets them right, so that's a Hammerton limit, not ours.
        // Found by this target; see fuzz/README.md.
        if !has_17_bit_step(&pcm) {
            let h = hammerton_alac_decode_packet(&fmtp, packet);
            assert_eq!(h, pcm, "Hammerton decoder disagrees");
        }
    }
});
