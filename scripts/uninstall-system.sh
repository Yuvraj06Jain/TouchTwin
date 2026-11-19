#!/usr/bin/env bash
# Remove everything install-system.sh set up (keys and paired phones included).
set -euo pipefail
systemctl --user disable --now touchtwind-mirror.service 2>/dev/null || true
rm -f "$HOME/.config/systemd/user/touchtwind-mirror.service"
systemctl --user daemon-reload
sudo systemctl disable --now touchtwind.service 2>/dev/null || true
sudo rm -f /etc/systemd/system/touchtwind.service \
  /etc/udev/rules.d/60-touchtwin-uinput.rules /etc/udev/rules.d/71-touchpad-touchtwin.rules \
  /etc/udev/rules.d/72-touchtwin-pad.rules \
  /usr/share/polkit-1/actions/dev.touchtwin.policy /etc/sysusers.d/touchtwin.conf \
  /usr/local/bin/touchtwind
sudo rm -rf /usr/local/share/doc/touchtwin /var/lib/touchtwin
sudo systemctl daemon-reload
sudo udevadm control --reload && sudo udevadm trigger --sysname-match=uinput
sudo userdel touchtwin 2>/dev/null || true
echo "TouchTwin removed."
