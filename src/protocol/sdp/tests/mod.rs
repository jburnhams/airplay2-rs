#![cfg(test)]

mod raop;

use super::*;

#[test]
fn test_parse_basic_sdp() {
    let sdp_text = r"v=0
o=iTunes 1234567890 1 IN IP4 192.168.1.100
s=iTunes
c=IN IP4 192.168.1.50
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
";

    let sdp = SdpParser::parse(sdp_text).unwrap();

    assert_eq!(sdp.version, 0);
    assert_eq!(sdp.session_name, "iTunes");
    assert_eq!(sdp.media.len(), 1);

    let audio = sdp.audio_media().unwrap();
    assert_eq!(audio.media_type, "audio");
    assert_eq!(audio.protocol, "RTP/AVP");
}

#[test]
fn test_parse_raop_announce() {
    let sdp_text = r"v=0
o=iTunes 3413821438 1 IN IP4 fe80::217:f2ff:fe0f:e0f6
s=iTunes
c=IN IP4 fe80::5a55:caff:fe1a:e288
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
a=rsaaeskey:ABCDEF123456
a=aesiv:0011223344556677
a=min-latency:11025
";

    let sdp = SdpParser::parse(sdp_text).unwrap();

    assert_eq!(sdp.rsaaeskey(), Some("ABCDEF123456"));
    assert_eq!(sdp.aesiv(), Some("0011223344556677"));
    assert_eq!(sdp.fmtp(), Some("96 352 0 16 40 10 14 2 255 0 0 44100"));
}

#[test]
fn test_parse_origin() {
    let sdp_text = "v=0\no=user 123 1 IN IP4 192.168.1.1\ns=test\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();

    let origin = sdp.origin.unwrap();
    assert_eq!(origin.username, "user");
    assert_eq!(origin.session_id, "123");
    assert_eq!(origin.addr_type, "IP4");
}

#[test]
fn test_builder() {
    let sdp_str = SdpBuilder::new()
        .origin("user", "123", "127.0.0.1")
        .session_name("test session")
        .media("audio", 0, "RTP/AVP", &["96"])
        .media_attribute("rtpmap", Some("96 AppleLossless"))
        .encode();

    assert!(sdp_str.contains("o=user 123 1 IN IP4 127.0.0.1"));
    assert!(sdp_str.contains("s=test session"));
    assert!(sdp_str.contains("m=audio 0 RTP/AVP 96"));
    assert!(sdp_str.contains("a=rtpmap:96 AppleLossless"));
}

#[test]
fn test_parse_invalid_version() {
    let sdp_text = "v=invalid\no=user 123 1 IN IP4 192.168.1.1\ns=test\n";
    let res = SdpParser::parse(sdp_text);
    assert!(matches!(res, Err(SdpParseError::InvalidVersion)));
}

#[test]
fn test_parse_invalid_origin() {
    let sdp_text = "v=0\no=user 123 1 IN IP4\ns=test\n";
    let res = SdpParser::parse(sdp_text);
    assert!(matches!(res, Err(SdpParseError::InvalidOrigin(_))));
}

#[test]
fn test_parse_invalid_connection() {
    let sdp_text = "v=0\nc=IN IP4\ns=test\n";
    let res = SdpParser::parse(sdp_text);
    assert!(matches!(res, Err(SdpParseError::InvalidConnection(_))));
}

#[test]
fn test_parse_invalid_media() {
    let sdp_text = "v=0\nm=audio 0 RTP/AVP\ns=test\n";
    let res = SdpParser::parse(sdp_text);
    assert!(matches!(res, Err(SdpParseError::InvalidMedia(_))));
}

#[test]
fn test_parse_empty_lines() {
    let sdp_text = "v=0\n\n\no=user 123 1 IN IP4 192.168.1.1\n\ns=test\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();
    assert_eq!(sdp.version, 0);
    assert_eq!(sdp.session_name, "test");
}

#[test]
fn test_parse_invalid_line_format() {
    // Missing `=` or unknown type
    let sdp_text = "v=0\ninvalidline\nx=unknown\ns=test\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();
    assert_eq!(sdp.version, 0);
    assert_eq!(sdp.session_name, "test");
}

#[test]
fn test_parse_attribute_without_value() {
    let sdp_text = "v=0\ns=test\na=recvonly\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();
    assert_eq!(sdp.attributes.get("recvonly"), Some(&None));
}

#[test]
fn test_parse_connection_for_current_media() {
    // Current parser doesn't save connection inside media, it just ignores it if current_media is
    // some. Let's at least ensure it parses and doesn't set global connection.
    let sdp_text = "v=0\ns=test\nm=audio 0 RTP/AVP 96\nc=IN IP4 192.168.1.50\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();
    assert!(sdp.connection.is_none());
}

#[test]
fn test_parse_multiple_media() {
    let sdp_text = "v=0\ns=test\nm=audio 0 RTP/AVP 96\nm=video 0 RTP/AVP 97\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();
    assert_eq!(sdp.media.len(), 2);
    assert_eq!(sdp.media[0].media_type, "audio");
    assert_eq!(sdp.media[1].media_type, "video");
}

#[test]
fn test_timing_parsing() {
    let sdp_text = "v=0\ns=test\nt=123 456\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();
    assert_eq!(sdp.timing, Some((123, 456)));

    let sdp_text = "v=0\ns=test\nt=invalid\n";
    let sdp = SdpParser::parse(sdp_text).unwrap();
    assert_eq!(sdp.timing, None); // Parts < 2
}
