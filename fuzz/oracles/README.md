# Reference implementations used as fuzzing oracles

These are other projects' C/C++ code, compiled into the fuzz targets so our
Rust code can be checked against implementations that work with real devices.
Vendored unmodified unless noted. Update by copying the files again from the
upstream commit and noting the new commit here.

| Directory | Upstream | Commit | Licence | Used by |
| --- | --- | --- | --- | --- |
| `alac/` | [mikebrady/alac](https://github.com/mikebrady/alac) `codec/` (Apple's ALAC reference, decoder files only) | `5d8c5db` | Apache 2.0 (`APPLE_LICENSE.txt`) | `alac_vs_shairport` |
| `shairport-sync/alac.[ch]` | [mikebrady/shairport-sync](https://github.com/mikebrady/shairport-sync) (David Hammerton's decoder) | `7ce90d8` | MIT-style, in file header | `alac_vs_shairport` |
| `shairport-sync/apple_alac.{cpp,h}` | same | `7ce90d8` | MIT, in file header. Include paths changed to `../alac/`. | `alac_vs_shairport` |
| `shairport-sync/raop_decrypt.c` | same, transcribed from `player.c` (`openssl_aes_decrypt_cbc` and the `ALAC_44100_S16_2` decrypt branch) | `7ce90d8` | MIT, in `player.c` header | `raop_audio_vs_shairport` |
| `pair_ap/pair-tlv.{c,h}` | [ejurgensen/pair_ap](https://github.com/ejurgensen/pair_ap) (pairing library shared by OwnTone and Shairport Sync) | `77b29bd` | MIT (`LICENSE`) | `tlv8_vs_pair_ap` |
| `pair_ap/pair-internal.h` | stub, written here | | | so `pair-tlv.c` builds without libsodium |
| `glue.c` | written here | | | thin C wrappers so Rust doesn't mirror C structs |

`libplist/` is [libimobiledevice/libplist](https://github.com/libimobiledevice/libplist)
2.3.0 (`7248021`), the C sources the library needs plus libcnary, under
LGPL-2.1 (`libplist/COPYING.LESSER`). `config.h` is written here. It is
compiled with `NDEBUG` because an `assert` in `plist_get_key_val` aborts on
dictionary keys with an embedded NUL. Used by `plist_vs_libplist`. It is only
linked into these fuzz binaries, never into the `airplay2` crate.

Shairport Sync's `player.c` licence header (applies to `raop_decrypt.c`):

> Copyright (c) James Laird 2011, 2013. Modifications copyright (c) Mike Brady
> 2014--2026. Permission is hereby granted, free of charge, to any person
> obtaining a copy of this software and associated documentation files (the
> "Software"), to deal in the Software without restriction, including without
> limitation the rights to use, copy, modify, merge, publish, distribute,
> sublicense, and/or sell copies of the Software, and to permit persons to
> whom the Software is furnished to do so, subject to the following
> conditions: The above copyright notice and this permission notice shall be
> included in all copies or substantial portions of the Software. THE
> SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
