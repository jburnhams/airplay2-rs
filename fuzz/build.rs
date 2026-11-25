//! Compiles the C/C++ reference implementations the fuzz targets compare against.
//! See `oracles/README.md` for where each file comes from.

fn main() {
    let ssl = pkg_config::probe_library("libcrypto")
        .expect("libcrypto not found: apt install libssl-dev (or brew install openssl)");

    let mut c = cc::Build::new();
    c.warnings(false)
        .file("oracles/glue.c")
        .file("oracles/pair_ap/pair-tlv.c")
        .file("oracles/shairport-sync/raop_decrypt.c")
        .file("oracles/shairport-sync/alac.c")
        .file("oracles/alac/ALACBitUtilities.c")
        .file("oracles/alac/EndianPortable.c")
        .file("oracles/alac/ag_dec.c")
        .file("oracles/alac/dp_dec.c")
        .file("oracles/alac/matrix_dec.c")
        .include("oracles")
        .include("oracles/libplist/include");
    for p in &ssl.include_paths {
        c.include(p);
    }
    c.compile("oracles_c");

    // libplist 2.3.0, built with NDEBUG: its assert in plist_get_key_val
    // aborts on keys with an embedded NUL, which fuzzing hits constantly.
    let mut lp = cc::Build::new();
    lp.warnings(false)
        .define("HAVE_CONFIG_H", None)
        .define("NDEBUG", None)
        .include("oracles/libplist")
        .include("oracles/libplist/include")
        .include("oracles/libplist/src")
        .include("oracles/libplist/libcnary/include");
    for f in [
        "plist",
        "bplist",
        "xplist",
        "jplist",
        "oplist",
        "jsmn",
        "base64",
        "bytearray",
        "hashtable",
        "ptrarray",
        "time64",
        "out-default",
        "out-limd",
        "out-plutil",
    ] {
        lp.file(format!("oracles/libplist/src/{f}.c"));
    }
    lp.file("oracles/libplist/libcnary/node.c")
        .file("oracles/libplist/libcnary/node_list.c")
        .compile("libplist_vendored");

    cc::Build::new()
        .cpp(true)
        .warnings(false)
        .file("oracles/alac/ALACDecoder.cpp")
        .file("oracles/shairport-sync/apple_alac.cpp")
        .include("oracles/alac")
        .compile("oracles_cpp");

    println!("cargo:rerun-if-changed=oracles");
}
