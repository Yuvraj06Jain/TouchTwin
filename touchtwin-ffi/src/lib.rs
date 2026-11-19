//! Kotlin-facing API of the TouchTwin core, generated with UniFFI.
//!
//! The Android app creates one [`TouchTwin`]. Pairing, TLS, discovery and reconnecting are
//! the shared Rust core, the same code the laptop runs. Methods that wait on the network
//! (`pair`) block: call them off the main thread. `touch` never blocks; it is called on
//! every touch event. Events arrive on a background thread through [`PadListener`].
//!
//! While a finger is down, the core sends a frame at least every [`HEARTBEAT`], so the
//! laptop can tell "holding still" from "connection lost". When the last finger lifts, the
//! empty frame is repeated, because a single lost "lift" datagram would leave a button held.

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

use tokio::{runtime::Runtime, task::JoinHandle};
use touchtwin_core::{
    lan::Lan,
    proto::{Contact, Msg, PadInfo, TouchFrame, CAP_TOUCHPAD, COORD_MAX, MAX_CONTACTS},
    Event, Fingerprint, Node, NodeConfig, PairingOffer, PeerRecord,
};

uniffi::setup_scaffolding!();

/// While fingers are down, send at least this often even without movement.
const HEARTBEAT: Duration = Duration::from_millis(100);
/// After the last finger lifts, send the empty frame this many more times…
const LIFT_REPEATS: u32 = 2;
/// …this far apart.
const LIFT_REPEAT_GAP: Duration = Duration::from_millis(15);

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum PadError {
    #[error("{0}")]
    Failed(String),
}

impl From<touchtwin_core::Error> for PadError {
    fn from(e: touchtwin_core::Error) -> Self {
        PadError::Failed(e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Laptop {
    /// Short id shown on both screens, e.g. `3F2A-91C0-77DE-0B14`.
    pub id: String,
    pub name: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum PadEvent {
    Paired { name: String },
    Connected { name: String },
    Disconnected,
    /// The laptop created its virtual touchpad: touches now move the pointer.
    PadReady,
    /// The laptop could not create its virtual touchpad (`message` says how to fix it).
    PadFailed { message: String },
}

/// One finger, as Android reports it. `x`, `y` are 0.0–1.0 across the pad.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Finger {
    pub slot: u8,
    /// New for every touch (Android's pointer id plus a counter, any u16).
    pub id: u16,
    pub x: f32,
    pub y: f32,
}

/// Implemented in Kotlin. Called on a background thread, one event at a time, in order.
#[uniffi::export(with_foreign)]
pub trait PadListener: Send + Sync {
    fn on_event(&self, event: PadEvent);
}

/// What the touch side shares with the heartbeat task.
#[derive(Default)]
struct TouchState {
    /// Laptop the pad drives (the most recently connected one).
    target: Option<Fingerprint>,
    /// The pad is open on the phone, at this size.
    pad: Option<PadInfo>,
    contacts: Vec<Contact>,
    last_sent: Option<Instant>,
}

#[derive(uniffi::Object)]
pub struct TouchTwin {
    rt: Runtime,
    node: Node,
    lan: Mutex<Option<Lan>>,
    keepers: Arc<Mutex<HashMap<Fingerprint, JoinHandle<()>>>>,
    touch: Arc<Mutex<TouchState>>,
    seq: Arc<AtomicU32>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn keep_connected(node: &Node, keepers: &Mutex<HashMap<Fingerprint, JoinHandle<()>>>, fp: Fingerprint) {
    let mut k = lock(keepers);
    if k.get(&fp).is_some_and(|h| !h.is_finished()) {
        return;
    }
    k.insert(fp, node.maintain(fp, vec![]));
}

fn laptop(node: &Node, p: &PeerRecord) -> Laptop {
    Laptop {
        id: p.fingerprint.short(),
        name: p.name.clone(),
        connected: node.is_connected(&p.fingerprint),
    }
}

fn send_frame(node: &Node, seq: &AtomicU32, st: &mut TouchState) -> bool {
    let Some(target) = st.target else { return false };
    if st.pad.is_none() {
        return false;
    }
    let frame = TouchFrame {
        seq: seq.fetch_add(1, Ordering::Relaxed),
        contacts: st.contacts.clone(),
    };
    st.last_sent = Some(Instant::now());
    node.send_datagram(&target, frame.encode()).unwrap_or(false)
}

fn send_pad_info(node: &Node, st: &TouchState) {
    if let (Some(target), Some(pad)) = (st.target, st.pad) {
        let _ = node.send(&target, Msg::PadInfo(pad).encode());
    }
}

fn to_contact(f: &Finger) -> Option<Contact> {
    if f.slot as usize >= MAX_CONTACTS || !f.x.is_finite() || !f.y.is_finite() {
        return None;
    }
    let scale = |v: f32| (v.clamp(0.0, 1.0) * COORD_MAX as f32).round() as u16;
    Some(Contact { slot: f.slot, id: f.id, x: scale(f.x), y: scale(f.y) })
}

#[uniffi::export]
impl TouchTwin {
    /// Start the node. `data_dir` should be the app's private files directory.
    #[uniffi::constructor]
    pub fn start(
        data_dir: String,
        device_name: String,
        listener: Arc<dyn PadListener>,
    ) -> Result<Arc<Self>, PadError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("touchtwin")
            .enable_all()
            .build()
            .map_err(|e| PadError::Failed(e.to_string()))?;
        let dir = PathBuf::from(data_dir);
        // The laptop never dials the phone, so any port will do.
        let (node, mut events) = rt.block_on(async {
            let mut last = None;
            for bind in [SocketAddr::from(([0u16; 8], 0)), SocketAddr::from(([0u8; 4], 0))] {
                match Node::start(NodeConfig::new(device_name.clone(), Some(dir.clone()), bind)).await {
                    Ok(x) => return Ok(x),
                    Err(e) => last = Some(e),
                }
            }
            Err(PadError::from(last.expect("tried at least once")))
        })?;

        let keepers = Arc::new(Mutex::new(HashMap::new()));
        let touch = Arc::new(Mutex::new(TouchState::default()));
        let seq = Arc::new(AtomicU32::new(0));
        let lan = {
            let _guard = rt.enter();
            for p in node.peers() {
                keep_connected(&node, &keepers, p.fingerprint);
            }
            let lan = Lan::new().ok();
            if let Some(l) = &lan {
                if let Err(e) = l.browse(node.clone()) {
                    tracing::warn!("mDNS browse failed: {e}");
                }
            }
            lan
        };

        // Events → Kotlin (and session bookkeeping).
        {
            let (node, touch, keepers) = (node.clone(), touch.clone(), keepers.clone());
            rt.spawn(async move {
                while let Some(ev) = events.recv().await {
                    let out = match ev {
                        Event::Paired { peer, name } => {
                            keep_connected(&node, &keepers, peer);
                            Some(PadEvent::Paired { name })
                        }
                        Event::PeerConnected { peer, name, capabilities, .. } => {
                            if !capabilities.iter().any(|c| c == CAP_TOUCHPAD) {
                                continue;
                            }
                            let mut st = lock(&touch);
                            st.target = Some(peer);
                            send_pad_info(&node, &st);
                            Some(PadEvent::Connected { name })
                        }
                        Event::PeerDisconnected { peer } => {
                            let mut st = lock(&touch);
                            if st.target == Some(peer) {
                                // Fall back to another connected laptop, if any.
                                st.target = node
                                    .peers()
                                    .into_iter()
                                    .map(|p| p.fingerprint)
                                    .find(|fp| *fp != peer && node.is_connected(fp));
                                send_pad_info(&node, &st);
                                Some(PadEvent::Disconnected)
                            } else {
                                None
                            }
                        }
                        Event::Message { body, .. } => match Msg::decode(&body) {
                            Some(Msg::PadStatus { ok: true, .. }) => Some(PadEvent::PadReady),
                            Some(Msg::PadStatus { ok: false, message }) => {
                                Some(PadEvent::PadFailed { message })
                            }
                            _ => None,
                        },
                        Event::Datagram { .. } => None,
                    };
                    if let Some(e) = out {
                        listener.on_event(e);
                    }
                }
            });
        }

        // Heartbeat: keep held fingers alive on the laptop.
        {
            let (node, touch, seq) = (node.clone(), touch.clone(), seq.clone());
            rt.spawn(async move {
                let mut tick = tokio::time::interval(HEARTBEAT / 2);
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tick.tick().await;
                    let mut st = lock(&touch);
                    if !st.contacts.is_empty() && st.last_sent.is_none_or(|t| t.elapsed() >= HEARTBEAT) {
                        send_frame(&node, &seq, &mut st);
                    }
                }
            });
        }

        Ok(Arc::new(Self {
            rt,
            node,
            lan: Mutex::new(lan),
            keepers,
            touch,
            seq,
        }))
    }

    /// This phone's id, as shown by `touchtwind devices` on the laptop.
    pub fn my_id(&self) -> String {
        self.node.fingerprint().short()
    }

    pub fn laptops(&self) -> Vec<Laptop> {
        self.node.peers().iter().map(|p| laptop(&self.node, p)).collect()
    }

    /// Pair with the laptop whose QR code was scanned. Blocks for up to ~20 s.
    pub fn pair(&self, uri: String) -> Result<Laptop, PadError> {
        let offer = PairingOffer::from_uri(&uri)?;
        let rec = self.rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(20), self.node.pair_with(&offer))
                .await
                .map_err(|_| {
                    PadError::Failed("the laptop did not answer; are you on the same network?".into())
                })?
                .map_err(PadError::from)
        })?;
        {
            let _guard = self.rt.enter();
            keep_connected(&self.node, &self.keepers, rec.fingerprint);
        }
        Ok(laptop(&self.node, &rec))
    }

    pub fn unpair(&self, laptop_id: String) -> Result<(), PadError> {
        let Some(p) = self.node.peers().into_iter().find(|p| p.fingerprint.short() == laptop_id) else {
            return Err(PadError::Failed("no such laptop".into()));
        };
        if let Some(h) = lock(&self.keepers).remove(&p.fingerprint) {
            h.abort();
        }
        self.node.unpair(&p.fingerprint)?;
        Ok(())
    }

    /// Retry connecting at once (app opened, network changed).
    pub fn reconnect_now(&self) {
        self.node.nudge();
    }

    /// The pad screen opened (or changed size). Physical size in millimetres.
    pub fn open_pad(&self, width_mm: f32, height_mm: f32) {
        let dmm = |mm: f32| (mm * 10.0).clamp(0.0, u16::MAX as f32) as u16;
        let mut st = lock(&self.touch);
        st.pad = Some(PadInfo { width_dmm: dmm(width_mm), height_dmm: dmm(height_mm) });
        st.contacts.clear();
        send_pad_info(&self.node, &st);
    }

    /// The pad screen closed: lift every finger on the laptop.
    pub fn close_pad(&self) {
        let mut st = lock(&self.touch);
        st.contacts.clear();
        if let Some(target) = st.target {
            let _ = self.node.send(&target, Msg::Release.encode());
        }
        st.pad = None;
    }

    /// Every touch event: the fingers that are down now (empty when the last one lifts).
    /// Returns false if nothing could be sent (not connected).
    pub fn touch(&self, fingers: Vec<Finger>) -> bool {
        let contacts: Vec<Contact> = fingers.iter().filter_map(to_contact).take(MAX_CONTACTS).collect();
        let mut st = lock(&self.touch);
        let lifted = contacts.is_empty() && !st.contacts.is_empty();
        st.contacts = contacts;
        let sent = send_frame(&self.node, &self.seq, &mut st);
        drop(st);
        if lifted {
            let (node, touch, seq) = (self.node.clone(), self.touch.clone(), self.seq.clone());
            self.rt.spawn(async move {
                for _ in 0..LIFT_REPEATS {
                    tokio::time::sleep(LIFT_REPEAT_GAP).await;
                    let mut st = lock(&touch);
                    if !st.contacts.is_empty() {
                        return; // a new touch started; its frames say everything
                    }
                    send_frame(&node, &seq, &mut st);
                }
            });
        }
        sent
    }

    /// Round-trip time to the laptop the pad drives, for a latency indicator.
    pub fn latency_ms(&self) -> Option<u32> {
        let target = lock(&self.touch).target?;
        self.node.rtt(&target).map(|d| d.as_millis() as u32)
    }

    pub fn shutdown(&self) {
        if let Some(l) = lock(&self.lan).take() {
            l.shutdown();
        }
        for (_, h) in lock(&self.keepers).drain() {
            h.abort();
        }
        self.rt.block_on(self.node.shutdown());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    struct Collect(Mutex<mpsc::Sender<PadEvent>>);
    impl PadListener for Collect {
        fn on_event(&self, event: PadEvent) {
            let _ = lock(&self.0).send(event);
        }
    }

    fn wait(rx: &mpsc::Receiver<PadEvent>, want: impl Fn(&PadEvent) -> bool) -> PadEvent {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let e = rx.recv_timeout(left).expect("event in time");
            if want(&e) {
                return e;
            }
        }
    }

    /// The whole phone side against a stand-in laptop: pair, open the pad, touch, lift.
    #[test]
    fn phone_pairs_opens_the_pad_and_streams_fingers() {
        let rt = Runtime::new().unwrap();
        let (laptop, mut lrx) = rt
            .block_on(Node::start(NodeConfig::new("Laptop", None, "127.0.0.1:0".parse().unwrap())))
            .unwrap();
        let offer = laptop.start_pairing(Duration::from_secs(60), vec![laptop.local_addr().unwrap()]);

        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let phone = TouchTwin::start(
            dir.path().to_string_lossy().into(),
            "Phone".into(),
            Arc::new(Collect(Mutex::new(tx))),
        )
        .unwrap();
        let l = phone.pair(offer.to_uri()).unwrap();
        assert_eq!(l.name, "Laptop");
        wait(&rx, |e| matches!(e, PadEvent::Connected { .. }));

        phone.open_pad(70.0, 150.0);
        // The laptop sees the pad size and answers that its pad is ready.
        let (pfp, info) = rt.block_on(async {
            loop {
                if let Some(Event::Message { peer, body }) = lrx.recv().await {
                    if let Some(Msg::PadInfo(i)) = Msg::decode(&body) {
                        return (peer, i);
                    }
                }
            }
        });
        assert_eq!(info, PadInfo { width_dmm: 700, height_dmm: 1500 });
        laptop
            .send(&pfp, Msg::PadStatus { ok: true, message: String::new() }.encode())
            .unwrap();
        wait(&rx, |e| matches!(e, PadEvent::PadReady));

        // Touch, hold still (heartbeat), lift (repeated).
        assert!(phone.touch(vec![Finger { slot: 0, id: 9, x: 0.5, y: 1.2 }]));
        std::thread::sleep(Duration::from_millis(350));
        assert!(phone.touch(vec![]));
        let frames: Vec<TouchFrame> = rt.block_on(async {
            let mut out = vec![];
            let _ = tokio::time::timeout(Duration::from_millis(800), async {
                while let Some(e) = lrx.recv().await {
                    if let Event::Datagram { body, .. } = e {
                        out.push(TouchFrame::decode(&body).expect("valid frame"));
                    }
                }
            })
            .await;
            out
        });
        let down: Vec<_> = frames.iter().filter(|f| !f.contacts.is_empty()).collect();
        let up = frames.iter().filter(|f| f.contacts.is_empty()).count();
        assert!(down.len() >= 3, "touch plus heartbeats while holding still: {}", down.len());
        assert_eq!(down[0].contacts[0], Contact { slot: 0, id: 9, x: 5000, y: COORD_MAX });
        assert_eq!(up, 1 + LIFT_REPEATS as usize, "the lift is repeated");
        let seqs: Vec<u32> = frames.iter().map(|f| f.seq).collect();
        let mut sorted = seqs.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), seqs.len(), "every frame has its own sequence number");

        phone.shutdown();
        rt.block_on(laptop.shutdown());
    }
}
