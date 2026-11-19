# Code copied from EchoTwin

TouchTwin reuses EchoTwin's security and pairing code instead of rewriting it. EchoTwin is GPL-3.0-or-later, so TouchTwin is too (see `LICENSE`).

The originals are in `EchoTwin/core/src/`. The commit they were copied from is recorded in `scripts/provenance/echotwin-commit`.

## Copied exactly (`touchtwin-core/src/`)

| File | What it does |
|---|---|
| `identity.rs` | Per-device Ed25519 key and self-signed certificate, identified by the certificate's SHA-256 fingerprint |
| `tls.rs` | Mutual TLS 1.3 inside QUIC. Only pinned fingerprints get in. No 0-RTT and no session resumption |
| `trust.rs` | Paired-device store (`peers.json`, mode 0600) and the single-use, time-limited pairing window |
| `pairing.rs` | The QR code format, HMAC proofs that bind both fingerprints, and the HKDF pair key |
| `discovery.rs` | Rotating HMAC tokens for mDNS, so the LAN never learns who you are |
| `lan.rs` | mDNS advertise and browse |
| `wire.rs` | Length-prefixed postcard frames on the control stream |
| `limiter.rs` | Handshake flood protection |
| `util.rs` | Atomic file writes and name sanitising |
| `probe.rs` | Finding a moved laptop through the gateway or the /24 when mDNS is blocked |
| `error.rs` | Error type |

Only two kinds of change were made:

1. **Rename.** `phonelink` became `touchtwin` and `EchoTwin` became `TouchTwin`. This changes the ALPN (`touchtwin/1`), the mDNS service (`_touchtwin._udp`), the QR prefix (`touchtwin:pair?`), and the HMAC/HKDF labels. The result is that an EchoTwin QR code, certificate exchange or discovery token can never be mistaken for a TouchTwin one, and the reverse is also true.
2. **`error.rs`.** The SQLite error variant was dropped, because TouchTwin has no database. The exact change is in `scripts/provenance/error.rs.diff`.

`scripts/check-provenance.sh` re-applies the rename to EchoTwin's originals and fails if any file differs in any other way. It also warns if EchoTwin's originals have changed since the copy, for example after a security fix there.

The unit tests inside these files were copied with them, and they pass unchanged.

## Derived, not copied

`node.rs` is EchoTwin's `node.rs` cut down. These parts are kept line for line:
- pairing (`start_pairing`, `pair_with`, `try_pair`, `serve_pairing`)
- dialling (`dial`, `connect_any`, `establish`)
- accepting (`accept_loop` with QUIC Retry and rate limiting, `handle_incoming`)
- the duplicate-connection rule
- reconnecting (`maintain`, `reconnect_candidates`)

These parts were removed: the durable outbox, the Bluetooth backup link, file streams, the pause switch, and the battery-saver timings.

One thing was added: QUIC datagrams for touch frames.

The attack tests from EchoTwin's `core/tests/attacker.rs` were ported to `touchtwin-core/tests/attacker.rs`.
