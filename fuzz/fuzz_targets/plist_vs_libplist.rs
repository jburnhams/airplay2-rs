//! Differential: our binary plist decoder vs libplist (C).
//!
//! Both must agree on whether the input is a valid binary plist, and when both
//! accept it they must produce the same value tree.
#![no_main]

use airplay2::protocol::plist::{PlistDecodeError, PlistValue, decode};
use airplay2_fuzz::{libplist_canonical, plist_canonical};
use libfuzzer_sys::fuzz_target;

/// libplist 2.3 aborts (an assert in `plist_get_key_val`) on a dictionary key
/// with an embedded NUL, so such inputs can't be compared.
fn has_nul_key(v: &PlistValue) -> bool {
    match v {
        PlistValue::Dictionary(d) => d.iter().any(|(k, v)| k.contains('\0') || has_nul_key(v)),
        PlistValue::Array(a) => a.iter().any(has_nul_key),
        _ => false,
    }
}

/// libplist 2.3 rejects an empty UTF-16 string (marker 0x60). We can't tell
/// from the decoded value which encoding an empty string used, so any empty
/// string in our result excuses a libplist rejection.
fn has_empty_string(v: &PlistValue) -> bool {
    match v {
        PlistValue::String(s) => s.is_empty(),
        PlistValue::Dictionary(d) => d.iter().any(|(k, v)| k.is_empty() || has_empty_string(v)),
        PlistValue::Array(a) => a.iter().any(has_empty_string),
        _ => false,
    }
}

fuzz_target!(|data: &[u8]| {
    let decoded = decode(data);
    if decoded.as_ref().is_ok_and(has_nul_key) {
        return;
    }
    let theirs = libplist_canonical(data);
    let invalid_utf8 = matches!(decoded, Err(PlistDecodeError::InvalidUtf8));
    let duplicate_key = matches!(decoded, Err(PlistDecodeError::DuplicateKey(_)));
    let int_overflow = matches!(decoded, Err(PlistDecodeError::IntegerOverflow));
    // Sets (marker 0xCn): libplist reads them as arrays; we don't support them.
    let set = matches!(decoded, Err(PlistDecodeError::InvalidObjectMarker(m)) if m >> 4 == 0xC);
    // We only accept the offset/ref sizes the format defines (1, 2, 4, 8);
    // libplist accepts any non-zero size.
    let odd_sizes = data.len() >= 32 && {
        let t = &data[data.len() - 32..];
        ![1, 2, 4, 8].contains(&t[6]) || ![1, 2, 4, 8].contains(&t[7])
    };
    let empty_string = decoded.as_ref().is_ok_and(has_empty_string);
    let err = decoded
        .as_ref()
        .err()
        .map(ToString::to_string)
        .unwrap_or_default();
    let ours = decoded.ok().map(|v| plist_canonical(&v));
    match (&ours, &theirs) {
        (Some(o), Some(t)) => assert_eq!(o, t, "value mismatch"),
        (None, None) => {}
        // libplist accepts the bplist00 null marker (0x00); we reject it
        // because PlistValue has no null and AirPlay never sends one.
        (None, Some(t)) if t.contains('n') => {}
        // libplist keeps invalid UTF-8 in 8-bit strings as raw bytes and
        // skips unpaired UTF-16 surrogates; a Rust String can hold neither,
        // so we reject the plist.
        (None, Some(_)) if invalid_utf8 => {}
        (Some(_), None) if empty_string => {}
        // libplist keeps both entries of a duplicated key; we reject.
        (None, Some(_)) if duplicate_key => {}
        // A 16-byte integer whose high 8 bytes aren't zero: libplist keeps the
        // low 8 bytes and drops the rest; we reject rather than truncate.
        (None, Some(_)) if int_overflow => {}
        (None, Some(_)) if odd_sizes => {}
        (None, Some(_)) if set => {}
        _ => panic!("acceptance mismatch: ours={ours:?} ({err}) libplist={theirs:?}"),
    }
});
