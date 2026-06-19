#!/usr/bin/env bash
# Creates a v4l2loopback virtual webcam device labelled "OpenDrop VJ".
# OpenDrop VJ auto-detects this device by label — no manual /dev/videoN needed.
#
# Prerequisites (Arch):   sudo pacman -S v4l2loopback-dkms ffmpeg
# Prerequisites (Ubuntu): sudo apt install v4l2loopback-dkms ffmpeg
#
# Usage: bash scripts/setup-v4l2.sh
#        (run once per session; the device persists until reboot or manual rmmod)

set -e

# Remove any existing v4l2loopback instance
sudo modprobe -r v4l2loopback 2>/dev/null || true

# Load with a fixed label and exclusive_caps so Chrome/OBS see it as a real webcam
sudo modprobe v4l2loopback devices=1 exclusive_caps=1 card_label="OpenDrop VJ"

# Find and print the resulting device path
DEV=""
for d in /sys/class/video4linux/video*; do
  if grep -q "OpenDrop VJ" "$d/name" 2>/dev/null; then
    DEV="/dev/$(basename "$d")"
    break
  fi
done

if [ -z "$DEV" ]; then
  echo "Erreur : device v4l2loopback non trouvé après modprobe." >&2
  exit 1
fi

echo "Webcam virtuelle prête : $DEV (label: OpenDrop VJ)"
echo "Lance OpenDrop VJ → ouvre l'Output → clique V4L2 ○"
