# TouchTwin

TouchTwin turns your phone's screen into your laptop's touchpad.

It doesn't use a fake mouse with its own idea of what a tap means. The laptop gets a virtual touchpad, and the phone streams your raw finger positions to it. Your laptop then handles the phone exactly like its real trackpad:

- **Click, double-click, drag, right-click, scroll** come from libinput's tap-to-click, tap-and-drag, two-finger tap and two-finger scroll.
- **Your gestures** come from InputActions (`~/.config/inputactions/config.yaml`). Edit that file and the phone follows at once. Nothing on the phone needs updating.
- **Your touchpad settings** (natural scrolling, speed, tap-and-drag and so on) are copied from the real touchpad through KWin. They're copied again whenever you change them in System Settings.

The security and pairing code comes from EchoTwin: mutual TLS 1.3 over QUIC with pinned keys, a single-use QR code that expires, and discovery tokens that don't identify you. See [PROVENANCE.md](PROVENANCE.md).

## Setup (laptop: KDE Plasma on Wayland)

Run this as yourself, not with sudo. It builds as you and asks for sudo only for the system parts:

```bash
scripts/install-system.sh
```

To remove everything again, including keys and paired phones:

```bash
scripts/uninstall-system.sh
```

## Security: who can create input

Only the TouchTwin service can create input devices, not every program you run.

| Piece | Runs as | Can do |
|---|---|---|
| `touchtwind.service` | the `touchtwin` system user | Open `/dev/uinput` (its group is the only one allowed), use the network, and talk to polkit. It is sandboxed by systemd: no home access, a read-only system, a syscall filter, and no other devices |
| `/var/lib/touchtwin` | owned by `touchtwin`, mode 0700 | Holds the laptop's key and the paired-phone list. Your programs can't read or change it, so they can't add themselves as a "phone" |
| `touchtwind pair` / `unpair` | you | Asks for your password through polkit (the KDE dialog). A background program can't see or answer that dialog |
| `touchtwind-mirror` (user service) | you, with no privileges | Only copies touchpad settings through KWin |
| Virtual touchpad device node | read-only for you | InputActions can follow the fingers, but nothing can write fake events into it |

Two things outside TouchTwin that you should know about:

- **InputActions' rule gives write access too.** `/etc/udev/rules.d/71-touchpad.rules`, added by the InputActions installer, gives you **read-write** access to every touchpad, including your real one. Writing to an evdev node injects events, so any program you run can already inject touchpad events into the real touchpad. InputActions only needs read access. TouchTwin's own rule removes the write access for the virtual touchpad only.
- **Root is unaffected.** Anything running as root can still do anything; this setup doesn't change that.

## Setup (phone: Android 10+)

```bash
scripts/build-android.sh
```

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

Building needs the Android SDK and NDK, JDK 17+ and rustup, the same as EchoTwin.

## Pairing

```bash
touchtwind pair
```

Scan the QR code with the TouchTwin app. The phone and laptop must be on the same network, for example the same Wi-Fi or the phone's hotspot. After pairing, the phone reconnects on its own when you open the app.

Other commands:

```bash
touchtwind devices
```

```bash
touchtwind unpair <name or fingerprint>
```

## How it works

```
phone (TouchPadView) --fingers: slot, id, x, y--> QUIC datagram --> touchtwind
    --> uinput multitouch touchpad --> libinput --> KWin + InputActions
```

- **Touch frames are snapshots** of every finger that is down, sent as unreliable QUIC datagrams. A lost frame is corrected by the next one, and the laptop ignores frames that arrive out of order.
- **Holding still:** while a finger is down the phone sends a frame at least every 100 ms. If the laptop hears nothing for 400 ms, it lifts every finger, so a dropped connection can never leave a drag stuck.
- **Lifting:** the "all fingers lifted" frame is sent three times.
- **Pad size:** the phone reports its physical size in mm, so movement distances and gesture thresholds work the same as on a real touchpad.

## Testing without the phone

```bash
cargo test --workspace
```

```bash
scripts/check-provenance.sh
```

To play gestures on a virtual touchpad without a network or phone (this needs the `touchtwin` group, so it runs as that user):

```bash
sudo -u touchtwin /usr/local/bin/touchtwind demo
```

To run the whole path, including pairing, the network and the installed service (your password is asked for once):

```bash
touchtwind pair --uri
```

```bash
cargo run -p touchtwin-ffi --example fake_phone -- pair '<the touchtwin:pair?… line>'
```

```bash
cargo run -p touchtwin-ffi --example fake_phone -- gestures
```

To watch what libinput sees while doing either:

```bash
sudo libinput debug-events
```
