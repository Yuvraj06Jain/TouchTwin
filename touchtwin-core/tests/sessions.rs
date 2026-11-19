//! Paired sessions: touch frames as datagrams, reliable messages, reconnecting.

mod common;

use std::time::Duration;

use common::*;
use touchtwin_core::{
    proto::{Contact, Msg, PadInfo, TouchFrame, CAP_TOUCHPAD},
    Event,
};

#[tokio::test]
async fn touch_frames_arrive_as_datagrams() {
    let (laptop, mut lrx, phone, _prx) = paired_pair().await;
    let lfp = laptop.fingerprint();
    assert!(phone.peer_supports(&lfp, CAP_TOUCHPAD));

    for seq in 0..20u32 {
        let f = TouchFrame {
            seq,
            contacts: vec![Contact { slot: 0, id: 1, x: seq as u16 * 100, y: 50 }],
        };
        assert!(phone.send_datagram(&lfp, f.encode()).unwrap());
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let mut seqs = vec![];
    tokio::time::timeout(WAIT, async {
        while seqs.len() < 20 {
            if let Some(Event::Datagram { peer, body }) = lrx.recv().await {
                assert_eq!(peer, phone.fingerprint());
                seqs.push(TouchFrame::decode(&body).expect("valid frame").seq);
            }
        }
    })
    .await
    .expect("frames arrive on loopback");
    seqs.sort();
    assert_eq!(seqs, (0..20).collect::<Vec<_>>());
}

#[tokio::test]
async fn reliable_messages_arrive_in_order() {
    let (laptop, _lrx, phone, mut prx) = paired_pair().await;
    let pfp = phone.fingerprint();
    let info = Msg::PadInfo(PadInfo { width_dmm: 700, height_dmm: 1500 });
    let status = Msg::PadStatus { ok: true, message: String::new() };
    assert!(laptop.send(&pfp, info.encode()).unwrap());
    assert!(laptop.send(&pfp, status.encode()).unwrap());
    let got = drain_messages(&mut prx, Duration::from_millis(500)).await;
    let got: Vec<Msg> = got.iter().filter_map(|b| Msg::decode(b)).collect();
    assert_eq!(got, vec![info, status]);
}

#[tokio::test]
async fn nothing_is_sent_without_a_session() {
    let (laptop, _l) = node("Laptop").await;
    let (phone, _p) = node("Phone").await;
    assert!(!phone.send(&laptop.fingerprint(), vec![1]).unwrap());
    assert!(!phone.send_datagram(&laptop.fingerprint(), vec![1]).unwrap());
    assert!(phone.send_datagram(&laptop.fingerprint(), vec![0; 4096]).is_err());
}

#[tokio::test]
async fn phone_reconnects_after_the_laptop_restarts() {
    let ldir = tempfile::tempdir().unwrap();
    let (laptop, mut lrx) = node_at("Laptop", Some(ldir.path().into()), "127.0.0.1:0").await;
    let port = laptop.local_addr().unwrap().port();
    let (phone, mut prx) = node("Phone").await;
    phone
        .pair_with(&laptop.start_pairing(Duration::from_secs(60), vec![laptop.local_addr().unwrap()]))
        .await
        .unwrap();
    let lfp = laptop.fingerprint();
    wait_for(&mut lrx, "connected", |e| matches!(e, Event::PeerConnected { .. })).await;
    let _keeper = phone.maintain(lfp, vec![]);

    laptop.shutdown().await;
    drop(laptop);
    drop(lrx);
    wait_for(&mut prx, "drop noticed", |e| matches!(e, Event::PeerDisconnected { .. })).await;

    // Same keys, same port: the phone's remembered address works again.
    let (laptop2, mut lrx2) =
        node_at("Laptop", Some(ldir.path().into()), &format!("127.0.0.1:{port}")).await;
    assert_eq!(laptop2.fingerprint(), lfp, "identity persisted");
    wait_for(&mut lrx2, "reconnected", |e| matches!(e, Event::PeerConnected { .. })).await;
    eventually("phone sees the session", || phone.is_connected(&lfp)).await;
    assert!(phone.send_datagram(&lfp, vec![0]).unwrap());
}

#[tokio::test]
async fn unpairing_ends_the_session_and_refuses_reconnects() {
    let (laptop, mut lrx, phone, _prx) = paired_pair().await;
    let pfp = phone.fingerprint();
    let lfp = laptop.fingerprint();
    assert!(laptop.unpair(&pfp).unwrap());
    wait_for(&mut lrx, "disconnected", |e| matches!(e, Event::PeerDisconnected { .. })).await;
    eventually("phone sees the drop", || !phone.is_connected(&lfp)).await;
    // The phone still trusts the laptop, but the laptop no longer admits the phone.
    assert!(phone.connect(lfp, laptop.local_addr().unwrap()).await.is_err());
}
