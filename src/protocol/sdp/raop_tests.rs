use super::parser::SdpParser;
use super::raop::*;
use crate::receiver::session::AudioCodec;

const SAMPLE_SDP: &str = r"v=0
o=iTunes 3413821438 0 IN IP4 192.168.1.100
s=iTunes
c=IN IP4 192.168.1.1
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
a=rsaaeskey:VGhpcyBpcyBhIHRlc3Qga2V5IHRoYXQgaXMgdXNlZCBmb3IgdGVzdGluZw==
a=aesiv:MDEyMzQ1Njc4OWFiY2RlZg==
a=min-latency:11025
";

const SIMPLE_SDP: &str = r"v=0
o=- 0 0 IN IP4 127.0.0.1
s=AirTunes
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
";

#[test]
fn test_detect_codec_alac() {
    let sdp = SdpParser::parse(SIMPLE_SDP).unwrap();
    let audio = sdp.audio_media().unwrap();

    let codec = detect_codec(audio).unwrap();
    assert_eq!(codec, AudioCodec::Alac);
}

#[test]
fn test_parse_alac_parameters() {
    let fmtp = "96 352 0 16 40 10 14 2 255 0 0 44100";
    let params = AlacParameters::parse(fmtp).unwrap();

    assert_eq!(params.frames_per_packet, 352);
    assert_eq!(params.bit_depth, 16);
    assert_eq!(params.channels, 2);
    assert_eq!(params.sample_rate, 44100);
}

#[test]
fn test_parse_alac_parameters_no_payload_type() {
    let fmtp = "352 0 16 40 10 14 2 255 0 0 44100";
    let params = AlacParameters::parse(fmtp).unwrap();

    assert_eq!(params.frames_per_packet, 352);
    assert_eq!(params.bit_depth, 16);
    assert_eq!(params.channels, 2);
    assert_eq!(params.sample_rate, 44100);
}

#[test]
fn test_parse_encryption_params() {
    let sdp = SdpParser::parse(SAMPLE_SDP).unwrap();
    let audio = sdp.audio_media().unwrap();

    let enc = parse_encryption(audio).unwrap();
    assert!(enc.is_some());

    let enc = enc.unwrap();
    assert!(!enc.encrypted_aes_key.is_empty());
    assert_eq!(enc.aes_iv.len(), 16);
}

#[test]
fn test_no_encryption() {
    let sdp = SdpParser::parse(SIMPLE_SDP).unwrap();
    let audio = sdp.audio_media().unwrap();

    let enc = parse_encryption(audio).unwrap();
    assert!(enc.is_none());
}

#[test]
fn test_extract_stream_params_unencrypted() {
    let sdp = SdpParser::parse(SIMPLE_SDP).unwrap();

    let params = extract_stream_parameters(&sdp, None).unwrap();

    assert_eq!(params.codec, AudioCodec::Alac);
    assert_eq!(params.sample_rate, 44100);
    assert_eq!(params.bits_per_sample, 16);
    assert_eq!(params.channels, 2);
    assert_eq!(params.frames_per_packet, 352);
    assert!(params.aes_key.is_none());
}

#[test]
fn test_pcm_codec() {
    let sdp_str = r"v=0
o=- 0 0 IN IP4 127.0.0.1
s=Test
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 L16/44100/2
";
    let sdp = SdpParser::parse(sdp_str).unwrap();
    let audio = sdp.audio_media().unwrap();

    let codec = detect_codec(audio).unwrap();
    assert_eq!(codec, AudioCodec::Pcm);
}

#[test]
fn test_min_latency_extraction() {
    let sdp = SdpParser::parse(SAMPLE_SDP).unwrap();
    let params = extract_stream_parameters(&sdp, None).unwrap();

    assert_eq!(params.min_latency, Some(11025));
}
