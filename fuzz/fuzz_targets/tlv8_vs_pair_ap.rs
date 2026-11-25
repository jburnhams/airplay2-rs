//! Differential: our pairing TLV8 decoder vs pair_ap (C, used by OwnTone and
//! Shairport Sync).
//!
//! Both must agree on whether the buffer parses, and on the value our
//! `TlvDecoder::get` returns for every type versus pair_ap's first item of that
//! type (pair_ap joins consecutive 255-byte fragments, as HAP specifies).
#![no_main]

use airplay2::protocol::pairing::tlv::{TlvDecoder, TlvType};
use airplay2_fuzz::pair_ap_parse;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let theirs = pair_ap_parse(data);
    let ours = TlvDecoder::decode(data).ok();
    match (&ours, &theirs) {
        (Some(o), Some(t)) => {
            for b in 0..=255u8 {
                let Some(ty) = TlvType::from_byte(b) else {
                    continue;
                };
                let ov = o.get(ty).map(<[u8]>::to_vec);
                assert_eq!(ov, t.first[b as usize], "value mismatch for type {b}");
            }
        }
        (None, None) => {}
        _ => panic!(
            "acceptance mismatch: ours={} pair_ap={}",
            ours.is_some(),
            theirs.is_some()
        ),
    }
});
