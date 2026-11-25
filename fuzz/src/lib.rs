//! FFI bindings to the C/C++ reference implementations in `oracles/`.

use std::ffi::{CStr, c_char, c_int, c_long};

unsafe extern "C" {
    fn pair_ap_tlv_parse(buf: *const u8, len: usize, lens: *mut c_long, vals: *mut u8) -> c_int;
    fn libplist_bin_canonical(buf: *const c_char, len: u32) -> *mut c_char;
    fn shairport_raop_decrypt(
        key: *const u8,
        iv: *const u8,
        data: *const u8,
        len: c_int,
        out: *mut u8,
    );
    fn apple_alac_init(fmtp: *mut i32) -> c_int;
    fn apple_alac_decode_frame(buf: *mut u8, len: u32, dest: *mut u8, outsize: *mut c_int)
    -> c_int;
    fn apple_alac_terminate() -> c_int;
}

/// pair_ap's view of a TLV8 buffer: for each type, the value of the first item
/// of that type (after it joined 255-byte fragments), plus the item count.
pub struct PairApTlv {
    pub first: Vec<Option<Vec<u8>>>,
    pub count: usize,
}

/// Parse TLV8 with pair_ap (OwnTone / Shairport Sync pairing library).
/// Returns `None` if pair_ap rejects the buffer.
pub fn pair_ap_parse(buf: &[u8]) -> Option<PairApTlv> {
    let mut lens = vec![0 as c_long; 256];
    let mut vals = vec![0u8; 256 * 65536];
    let n = unsafe {
        pair_ap_tlv_parse(
            buf.as_ptr(),
            buf.len(),
            lens.as_mut_ptr(),
            vals.as_mut_ptr(),
        )
    };
    if n < 0 {
        return None;
    }
    let first = (0..256)
        .map(|t| {
            let l = lens[t];
            (l >= 0).then(|| {
                let l = (l as usize).min(65536);
                vals[t * 65536..t * 65536 + l].to_vec()
            })
        })
        .collect();
    Some(PairApTlv {
        first,
        count: n as usize,
    })
}

/// Parse a binary plist with libplist and return its canonical text form
/// (see `oracles/glue.c`). Returns `None` if libplist rejects it.
pub fn libplist_canonical(buf: &[u8]) -> Option<String> {
    let len = u32::try_from(buf.len()).ok()?;
    let p = unsafe { libplist_bin_canonical(buf.as_ptr().cast(), len) };
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { libc_free(p.cast()) };
    Some(s)
}

unsafe extern "C" {
    #[link_name = "free"]
    fn libc_free(p: *mut std::ffi::c_void);
}

/// Decrypt one RAOP audio payload exactly as Shairport Sync does.
pub fn shairport_decrypt(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; data.len()];
    unsafe {
        shairport_raop_decrypt(
            key.as_ptr(),
            iv.as_ptr(),
            data.as_ptr(),
            data.len() as c_int,
            out.as_mut_ptr(),
        );
    }
    out
}

/// Decode one ALAC packet with Apple's decoder, configured from an SDP fmtp
/// line the way Shairport Sync's `apple_alac_init` does.
/// Returns interleaved little-endian 16-bit PCM bytes.
pub fn apple_alac_decode(fmtp: &[i32; 12], packet: &[u8]) -> Vec<u8> {
    let mut fmtp = *fmtp;
    let frames = 352usize;
    let channels = fmtp[7].max(1) as usize;
    let mut out = vec![0u8; frames * channels * 4];
    let mut input = packet.to_vec();
    input.extend_from_slice(&[0u8; 16]); // decoder may read a little past the end
    let mut n: c_int = 0;
    unsafe {
        apple_alac_init(fmtp.as_mut_ptr());
        apple_alac_decode_frame(
            input.as_mut_ptr(),
            packet.len() as u32,
            out.as_mut_ptr(),
            &mut n,
        );
        apple_alac_terminate();
    }
    out.truncate(n.max(0) as usize * channels * 2);
    out
}

unsafe extern "C" {
    fn hammerton_alac_decode(fmtp: *const i32, input: *mut u8, out: *mut u8) -> c_int;
}

/// Decode one 16-bit stereo ALAC packet with Hammerton's decoder, which
/// Shairport Sync uses by default. Returns interleaved LE 16-bit PCM bytes.
pub fn hammerton_alac_decode_packet(fmtp: &[i32; 12], packet: &[u8]) -> Vec<u8> {
    let mut input = packet.to_vec();
    input.extend_from_slice(&[0u8; 64]);
    let mut out = vec![0u8; 352 * 2 * 4];
    let n = unsafe { hammerton_alac_decode(fmtp.as_ptr(), input.as_mut_ptr(), out.as_mut_ptr()) };
    out.truncate(n.max(0) as usize);
    out
}

/// Render our `PlistValue` in the same canonical form as `oracles/glue.c`.
pub fn plist_canonical(v: &airplay2::protocol::plist::PlistValue) -> String {
    use airplay2::protocol::plist::PlistValue as P;
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
    match v {
        P::Boolean(b) => {
            if *b {
                "b1".into()
            } else {
                "b0".into()
            }
        }
        P::Integer(i) if *i < 0 => format!("i{i}"),
        P::Integer(i) => format!("i{}", *i as u64),
        P::UnsignedInteger(u) => format!("i{u}"),
        P::Real(r) => format!("r{:016x}", r.to_bits()),
        // libplist hands strings back NUL-terminated, so it can't report
        // anything after an embedded NUL. Compare only up to it.
        P::String(s) => format!("s{}", hex(s.split('\0').next().unwrap_or("").as_bytes())),
        P::Data(d) => format!("x{}", hex(d)),
        P::Date(_) => "t".into(),
        P::Uid(u) => format!("u{u}"),
        P::Array(a) => format!(
            "a[{}]",
            a.iter().map(plist_canonical).collect::<Vec<_>>().join(",")
        ),
        P::Dictionary(d) => {
            let mut kv: Vec<(String, String)> = d
                .iter()
                .map(|(k, v)| (hex(k.as_bytes()), plist_canonical(v)))
                .collect();
            kv.sort();
            format!(
                "d{{{}}}",
                kv.into_iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}
