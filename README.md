# airplay2-rs

An attempt at a pure Rust AirPlay 1 and 2 audio sender. **Not usable yet.**

## Status (2026-11-25)

- Streams PCM and ALAC to the [openairplay Python receiver](airplay2-receiver/) on
  loopback, verified by the end-to-end tests in [`integration_tests/`](integration_tests/).
- Has **never** played audio on a real speaker (HomePod, AirPort Express, Sonos and
  others were tried).
- Much of the code and many of the tests were machine-generated and are being
  audited. Expect large parts to be rewritten or removed.

The plan is to rebuild a small core that is tested against independent
implementations (shairport-sync, OwnTone, pyatv) and real hardware before anything
is called working. Device notes live in [`docs/devices/`](docs/devices/).

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
