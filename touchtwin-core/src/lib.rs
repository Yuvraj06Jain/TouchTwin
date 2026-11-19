//! TouchTwin core: everything both the Linux daemon and the Android app share.
//!
//! The security modules are copied from EchoTwin (see `PROVENANCE.md`):
//! * [`identity`]: per-device Ed25519 key + self-signed certificate, identified by its SHA-256 fingerprint.
//! * [`pairing`]: QR-code pairing; a one-time secret shown only on the laptop screen.
//! * [`tls`]: mutual TLS 1.3 over QUIC that only admits pinned fingerprints.
//! * [`discovery`] / [`lan`]: rotating opaque mDNS tokens so the LAN never learns who you are.
//! * [`limiter`]: handshake flood protection for hostile networks.
//!
//! New in TouchTwin:
//! * [`node`]: endpoint, sessions, reconnect, plus QUIC datagrams for touch frames.
//! * [`proto`]: the touchpad messages.

pub mod discovery;
pub mod error;
pub mod identity;
pub mod lan;
pub mod limiter;
pub mod node;
pub mod pairing;
pub mod probe;
pub mod proto;
pub mod tls;
pub mod trust;
mod util;
pub mod wire;

pub use error::{Error, Result};
pub use identity::{Fingerprint, Identity};
pub use node::{Event, Node, NodeConfig};
pub use pairing::PairingOffer;
pub use trust::PeerRecord;
pub use wire::PROTOCOL_VERSION;
