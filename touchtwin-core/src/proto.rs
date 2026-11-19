//! The touchpad protocol on top of a [`crate::Node`] session.
//!
//! The phone does not interpret gestures. It streams the raw fingers it sees, and the laptop
//! replays them on a virtual touchpad, so the laptop's own touchpad stack (libinput, KWin,
//! InputActions) decides what a tap, drag, scroll or 3-finger swipe means.
//!
//! * [`Msg`] travels reliably on the control stream (`Node::send`).
//! * [`TouchFrame`] travels as an unreliable QUIC datagram (`Node::send_datagram`). Every
//!   frame is a full snapshot of the fingers that are down, so a lost frame is corrected by
//!   the next one; the receiver ignores frames older than the newest it applied.
//!
//! Compatibility: like `wire`, variants and fields are only ever appended.

use serde::{Deserialize, Serialize};

/// Both sides support the touchpad protocol below.
pub const CAP_TOUCHPAD: &str = "touchpad/1";

/// Most fingers a frame may carry (libinput understands up to five).
pub const MAX_CONTACTS: usize = 5;

/// Touch coordinates run from 0 to this value on both axes; [`PadInfo`] gives the
/// physical size, so the laptop knows the aspect ratio and resolution.
pub const COORD_MAX: u16 = 10_000;

/// Reliable messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    /// Phone → laptop, at the start of every session and whenever the pad changes size
    /// (rotation, split screen). The laptop (re)creates its virtual touchpad to match.
    PadInfo(PadInfo),
    /// Phone → laptop: the pad screen was closed; lift every finger now.
    Release,
    /// Laptop → phone: the virtual touchpad is ready (`ok`) or could not be created.
    PadStatus { ok: bool, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PadInfo {
    /// Physical size of the touch area, in tenths of a millimetre.
    pub width_dmm: u16,
    pub height_dmm: u16,
}

impl PadInfo {
    /// Sizes outside 2 cm – 40 cm are not a phone screen; clamp them.
    pub fn sanitized(self) -> Self {
        Self {
            width_dmm: self.width_dmm.clamp(200, 4000),
            height_dmm: self.height_dmm.clamp(200, 4000),
        }
    }
}

/// One finger. `slot` is stable while the finger is down (0..MAX_CONTACTS); `id` is new
/// for every touch, so lifting and putting a finger back down is seen as a new touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub slot: u8,
    pub id: u16,
    pub x: u16,
    pub y: u16,
}

/// Snapshot of all fingers on the pad.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TouchFrame {
    /// Increases by one per frame (wrapping), within one session.
    pub seq: u32,
    pub contacts: Vec<Contact>,
}

impl Msg {
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("Msg always serialises")
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        postcard::from_bytes(bytes).ok()
    }
}

impl TouchFrame {
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("TouchFrame always serialises")
    }

    /// Decodes and validates: too many fingers, slots out of range, duplicate slots or
    /// coordinates beyond [`COORD_MAX`] are refused (the peer is paired, but be strict).
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let f: TouchFrame = postcard::from_bytes(bytes).ok()?;
        if f.contacts.len() > MAX_CONTACTS {
            return None;
        }
        let mut seen = 0u8;
        for c in &f.contacts {
            if c.slot as usize >= MAX_CONTACTS || c.x > COORD_MAX || c.y > COORD_MAX {
                return None;
            }
            if seen & (1 << c.slot) != 0 {
                return None;
            }
            seen |= 1 << c.slot;
        }
        Some(f)
    }
}

/// `a` is newer than `b` in wrapping sequence order.
pub fn seq_newer(a: u32, b: u32) -> bool {
    let d = a.wrapping_sub(b);
    d != 0 && d < u32::MAX / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(slot: u8, x: u16) -> Contact {
        Contact { slot, id: slot as u16 + 1, x, y: 5 }
    }

    #[test]
    fn frames_roundtrip_and_stay_small() {
        let f = TouchFrame {
            seq: 123_456,
            contacts: (0..5).map(|s| c(s, 9_999)).collect(),
        };
        let b = f.encode();
        assert!(b.len() < 64, "five fingers fit easily in a datagram: {}", b.len());
        assert_eq!(TouchFrame::decode(&b), Some(f));
    }

    #[test]
    fn bad_frames_are_refused() {
        let six = TouchFrame { seq: 1, contacts: (0..6).map(|s| c(s % 5, 1)).collect() };
        assert_eq!(TouchFrame::decode(&six.encode()), None);
        let dup = TouchFrame { seq: 1, contacts: vec![c(1, 1), c(1, 2)] };
        assert_eq!(TouchFrame::decode(&dup.encode()), None);
        let far = TouchFrame { seq: 1, contacts: vec![c(0, COORD_MAX + 1)] };
        assert_eq!(TouchFrame::decode(&far.encode()), None);
        let slot = TouchFrame { seq: 1, contacts: vec![c(5, 1)] };
        assert_eq!(TouchFrame::decode(&slot.encode()), None);
        assert_eq!(TouchFrame::decode(b"\xff\xff"), None);
    }

    #[test]
    fn sequence_order_wraps() {
        assert!(seq_newer(2, 1));
        assert!(!seq_newer(1, 2));
        assert!(!seq_newer(7, 7));
        assert!(seq_newer(0, u32::MAX));
        assert!(seq_newer(3, u32::MAX - 3));
    }

    #[test]
    fn msgs_roundtrip() {
        for m in [
            Msg::PadInfo(PadInfo { width_dmm: 700, height_dmm: 1500 }),
            Msg::Release,
            Msg::PadStatus { ok: false, message: "no uinput".into() },
        ] {
            assert_eq!(Msg::decode(&m.encode()), Some(m));
        }
    }
}
