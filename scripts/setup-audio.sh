#!/usr/bin/env bash
# Creates a PipeWire virtual source that captures system audio output.
# Run once per session (or add to autostart).
# The device appears as "OpenDrop - Son du PC" in any browser's audio picker.

MONITOR=$(pactl list sources short | grep '\.monitor' | grep -v 'opendrop' | awk '{print $2}' | head -1)

if [ -z "$MONITOR" ]; then
  echo "No audio output monitor found. Is PipeWire running?"
  exit 1
fi

# Remove existing virtual source if already loaded
pactl list modules short | grep 'opendrop_virtmic' | awk '{print $1}' | xargs -r -I{} pactl unload-module {}

pactl load-module module-virtual-source \
  source_name=opendrop_virtmic \
  master="$MONITOR" \
  source_properties=device.description="OpenDrop\ -\ Son\ du\ PC"

echo "Audio source ready: OpenDrop - Son du PC (master: $MONITOR)"
