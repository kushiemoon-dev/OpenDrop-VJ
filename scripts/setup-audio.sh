#!/usr/bin/env bash
# Creates a PipeWire virtual source that captures system audio output.
# Run once per session (or add to autostart).
# The device appears as "OpenDrop - Son du PC" in any browser's audio picker.

DEFAULT_SINK=$(pactl get-default-sink)
MONITOR="${DEFAULT_SINK}.monitor"

if [ -z "$DEFAULT_SINK" ] || ! pactl list sources short | grep -q "$MONITOR"; then
  echo "No monitor found for default sink ($DEFAULT_SINK). Is PipeWire running?"
  exit 1
fi

# Remove existing virtual source if already loaded
pactl list modules short | grep 'opendrop_virtmic' | awk '{print $1}' | xargs -r -I{} pactl unload-module {}

pactl load-module module-virtual-source \
  source_name=opendrop_virtmic \
  master="$MONITOR" \
  source_properties=device.description="OpenDrop\ -\ Son\ du\ PC"

echo "Audio source ready: OpenDrop - Son du PC (master: $MONITOR)"
