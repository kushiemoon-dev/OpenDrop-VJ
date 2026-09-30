#!/usr/bin/env bash
# Build the PipeWire client library from source into a prefix.
#
# Usage: build-pipewire.sh <version> <prefix>
#
# Why not apt: Ubuntu 22.04 ships libpipewire 0.3.48, which is too old for the
# `pipewire` crate that cpal 0.18.2 pulls in (libspa bindings fail to compile).
# Only the client side is built: no daemon extras, no ALSA/JACK/GStreamer
# plugins, no docs. The library lands in <prefix>/lib (not a multiarch dir) so
# pkg-config and the SPA plugin paths stay simple.
set -euo pipefail

VERSION="${1:?usage: build-pipewire.sh <version> <prefix>}"
PREFIX="${2:?usage: build-pipewire.sh <version> <prefix>}"

for tool in meson ninja curl tar; do
    command -v "$tool" >/dev/null || { echo "error: '$tool' not found" >&2; exit 1; }
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

curl -fsSL "https://gitlab.freedesktop.org/pipewire/pipewire/-/archive/${VERSION}/pipewire-${VERSION}.tar.gz" \
    | tar -xz -C "$WORK"

cd "$WORK/pipewire-${VERSION}"
meson setup build \
    --prefix="$PREFIX" --libdir=lib -Dbuildtype=release \
    -Dtests=disabled -Dexamples=disabled -Dman=disabled -Ddocs=disabled \
    -Dgstreamer=disabled -Dpipewire-alsa=disabled -Dpipewire-jack=disabled \
    -Dpipewire-v4l2=disabled -Dsystemd=disabled -Dsession-managers=[] \
    -Dalsa=disabled -Dbluez5=disabled -Djack=disabled -Dv4l2=disabled \
    -Dlibcamera=disabled -Dvulkan=disabled -Dffmpeg=disabled -Dudev=disabled \
    -Ddbus=disabled -Dsdl2=disabled -Dsndfile=disabled -Dlibpulse=disabled \
    -Davahi=disabled -Dlibusb=disabled -Droc=disabled -Draop=disabled \
    -Dlv2=disabled -Dx11=disabled -Dx11-xfixes=disabled -Dlibcanberra=disabled \
    -Dreadline=disabled -Dpw-cat=disabled -Dflatpak=disabled
meson compile -C build
meson install -C build
