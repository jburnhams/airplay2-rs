# Device logs

Notes from testing against real hardware, January to March 2026.

**Read with care.** No sound was ever heard from any real device in that period
(confirmed by Jonathan, 2026-11-25). Commit messages and notes that say audio
played are wrong. What *is* useful here is the observed device behaviour:
response codes, SETUP replies, and the HomePod PTP quirks.

| File | What it covers |
| --- | --- |
| [homepod-gen1-log.md](homepod-gen1-log.md) | HomePod gen 1 (`AudioAccessory1,1`) and AirPort Express sessions: SETUP, PTP, RECORD, volume |
| [pairing-attempts-2026-01.md](pairing-attempts-2026-01.md) | Transient pairing against Sonos One, Samsung HW-Q990C, AirPort Express, UxPlay |

The PTP fixes worth keeping are best described in the commit messages of
`5bd8986`, `fb20a7f` and `4e9957f` (2026-03-11 to 2026-03-13).
