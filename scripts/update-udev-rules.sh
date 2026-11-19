#!/usr/bin/env bash
# Reinstall only TouchTwin's udev rules (after they change) and recreate the virtual pad.
set -euo pipefail
cd "$(dirname "$0")/.."
ME="$(id -un)"
sudo install -Dm644 packaging/60-touchtwin-uinput.rules /etc/udev/rules.d/60-touchtwin-uinput.rules
sudo install -Dm644 packaging/71-touchpad-touchtwin.rules /etc/udev/rules.d/71-touchpad-touchtwin.rules
sed "s/@USER@/$ME/g" packaging/72-touchtwin-pad.rules.in | sudo tee /etc/udev/rules.d/72-touchtwin-pad.rules >/dev/null
sudo udevadm control --reload
sudo systemctl restart touchtwind.service
echo "udev rules updated; the virtual pad is recreated when the phone next connects."
