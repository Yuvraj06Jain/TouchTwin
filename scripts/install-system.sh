#!/usr/bin/env bash
# Install the laptop side of TouchTwin.
#
# Run as yourself (not with sudo): it builds as you, then asks for sudo only for the
# system parts. What it sets up:
#   * a `touchtwin` system user; its group is the ONLY one that can open /dev/uinput
#   * touchtwind.service, running as that user, with its keys in /var/lib/touchtwin
#   * a polkit action: pairing or forgetting a phone asks for your password
#   * the virtual touchpad's device node readable (not writable) by you, for InputActions
#   * touchtwind-mirror (user service, no privileges): copies your touchpad settings
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ $EUID -eq 0 ]]; then
  echo "Run this as your normal user; it uses sudo where needed."; exit 1
fi
ME="$(id -un)"

echo "==> Building touchtwind"
cargo build --release -p touchtwind

echo "==> Installing (sudo)"
sudo install -Dm755 target/release/touchtwind /usr/local/bin/touchtwind
sudo install -Dm644 README.md /usr/local/share/doc/touchtwin/README.md
sudo install -Dm644 packaging/touchtwin.sysusers /etc/sysusers.d/touchtwin.conf
sudo systemd-sysusers /etc/sysusers.d/touchtwin.conf
sudo install -Dm644 packaging/dev.touchtwin.policy /usr/share/polkit-1/actions/dev.touchtwin.policy
sudo install -Dm644 packaging/60-touchtwin-uinput.rules /etc/udev/rules.d/60-touchtwin-uinput.rules
sudo install -Dm644 packaging/71-touchpad-touchtwin.rules /etc/udev/rules.d/71-touchpad-touchtwin.rules
sed "s/@USER@/$ME/g" packaging/72-touchtwin-pad.rules.in |
  sudo tee /etc/udev/rules.d/72-touchtwin-pad.rules >/dev/null
sudo udevadm control --reload
sudo udevadm trigger --sysname-match=uinput
sudo install -Dm644 packaging/touchtwind.service /etc/systemd/system/touchtwind.service
sudo systemctl daemon-reload
sudo systemctl enable touchtwind.service
sudo systemctl restart touchtwind.service

echo "==> Settings mirror (user service)"
install -Dm644 packaging/touchtwind-mirror.service "$HOME/.config/systemd/user/touchtwind-mirror.service"
systemctl --user daemon-reload
systemctl --user enable touchtwind-mirror.service
systemctl --user restart touchtwind-mirror.service

echo
echo "==> Checks"
sleep 1
ls -l /dev/uinput
if [[ -w /dev/uinput ]]; then
  echo "WARNING: you can still write /dev/uinput; another udev rule grants it. Look for 'uinput' in /etc/udev/rules.d and /usr/lib/udev/rules.d."
else
  echo "ok: your programs cannot open /dev/uinput; only the touchtwind service can."
fi
if systemctl is-active --quiet touchtwind.service; then
  echo "ok: touchtwind.service is running"
else
  echo "touchtwind.service failed:"; sudo journalctl -u touchtwind -n 20 --no-pager; exit 1
fi
if command -v ufw >/dev/null && sudo ufw status | grep -q "Status: active"; then
  echo "note: ufw is active; let the phone in with: sudo ufw allow 47830/udp"
fi
echo
echo "Pair your phone with:  touchtwind pair"
