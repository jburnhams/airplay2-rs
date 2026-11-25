//! Differential: our RAOP (AirPlay 1) audio packet encryption vs Shairport
//! Sync's decryption.
//!
//! Builds packets with `RaopStreamer::encode_frame` (the path the AirPlay 1
//! session uses), strips the 12-byte RTP header and decrypts the payload the
//! way Shairport Sync does. The result must equal the payload we put in.
#![no_main]

use airplay2::protocol::raop::RaopSessionKeys;
use airplay2::streaming::raop_streamer::{RaopStreamConfig, RaopStreamer};
use airplay2_fuzz::shairport_decrypt;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (Vec<Vec<u8>>, u8)| {
    let (payloads, _) = input;
    let keys = RaopSessionKeys::generate().unwrap();
    let mut streamer = RaopStreamer::new(&keys, RaopStreamConfig::default());
    for payload in payloads.iter().take(8) {
        let packet = streamer.encode_frame(payload);
        let body = &packet[12..];
        let decrypted = shairport_decrypt(keys.aes_key(), keys.aes_iv(), body);
        assert_eq!(
            &decrypted, payload,
            "Shairport Sync would decrypt a different payload"
        );
    }
});
