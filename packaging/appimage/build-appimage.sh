#!/usr/bin/env bash
# Assembles the AppDir and invokes appimagetool to produce the Linux
# AppImage for OpenDrop-Native.
#
# Usage: packaging/appimage/build-appimage.sh
# Prerequisite: `cargo build --release` (this script does not build Rust).
#
# Output: OpenDrop-Native-<version>-x86_64.AppImage at the repo root,
# where <version> is read from app/Cargo.toml's [package].version.
#
# Shared-library bundling policy (Linux AppImage):
#   Bundled into usr/lib/ (copied from the real file `ldd` resolves to,
#   following symlinks, with a same-named symlink kept alongside so the
#   SONAME the binary was linked against still resolves):
#     - libndi.so.6          (NDI SDK has no distro package)
#     - libprojectM-4.so.4   (projectM has no distro package)
#     - libavahi-client.so.3 (avahi is opt-in on a standard Hyprland setup)
#     - libavahi-common.so.3 (same as above)
#   Never bundled: libGLESv2.so.2, libGLdispatch.so.0: these are part of
#   the GPU driver stack (libglvnd/Mesa dispatch), not portable software.
#   Bundling a driver-linked .so ties the AppImage to the exact driver of
#   the build machine and breaks rendering on any other GPU driver. This
#   is a well-known AppImage-with-OpenGL pitfall.
#   Also not bundled (treated as base-system on a standard Arch/Hyprland/
#   Wayland target): libpipewire, libasound, libdbus-1, libsystemd,
#   libssl/libcrypto.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

BINARY="$REPO_ROOT/target/release/opendrop-app"
APPDIR="$SCRIPT_DIR/AppDir"
CACHE_DIR="$SCRIPT_DIR/.cache"
APPIMAGETOOL="$CACHE_DIR/appimagetool"
# Real directory on this build machine holding the 9795-file preset pack.
# Not part of the repo; there is no other source for it to read from.
PRESETS_SRC="/srv/http/opendrop-presets"
CARGO_TOML="$REPO_ROOT/app/Cargo.toml"

if [[ ! -x "$BINARY" ]]; then
    echo "error: release binary not found at $BINARY (run 'cargo build --release' first)" >&2
    exit 1
fi

if [[ ! -d "$PRESETS_SRC" ]]; then
    echo "error: presets source directory not found: $PRESETS_SRC" >&2
    exit 1
fi

VERSION=$(awk -F'"' '
    /^\[package\]/ { in_package=1; next }
    /^\[/ { in_package=0 }
    in_package && /^version[[:space:]]*=/ { print $2; exit }
' "$CARGO_TOML")

if [[ -z "$VERSION" ]]; then
    echo "error: could not read [package].version from $CARGO_TOML" >&2
    exit 1
fi

OUTPUT_PATH="$REPO_ROOT/OpenDrop-Native-${VERSION}-x86_64.AppImage"

# --- 1. Fetch appimagetool and its runtime (no pacman/AUR package on this machine) ---

if ! command -v jq >/dev/null 2>&1; then
    echo "error: jq is required to parse the GitHub releases API response" >&2
    exit 1
fi

mkdir -p "$CACHE_DIR"

if [[ ! -x "$APPIMAGETOOL" ]]; then
    echo "Downloading appimagetool (latest release)..."
    DOWNLOAD_URL=$(curl -fsSL https://api.github.com/repos/AppImage/appimagetool/releases/latest \
        | jq -r '.assets[] | select(.name == "appimagetool-x86_64.AppImage") | .browser_download_url')
    if [[ -z "$DOWNLOAD_URL" ]]; then
        echo "error: could not find an x86_64 appimagetool asset in the latest GitHub release" >&2
        exit 1
    fi
    curl -fsSL -o "$APPIMAGETOOL" "$DOWNLOAD_URL"
    chmod +x "$APPIMAGETOOL"
fi

# appimagetool tries to download this itself on every run and it fails in
# this environment's network setup ("server returned status code 0"), even
# though a plain curl to the same host succeeds. Fetch it once ourselves and
# pass it via --runtime-file so appimagetool never attempts that download.
RUNTIME_FILE="$CACHE_DIR/runtime-x86_64"
if [[ ! -f "$RUNTIME_FILE" ]]; then
    echo "Downloading AppImage runtime (latest release)..."
    RUNTIME_URL=$(curl -fsSL https://api.github.com/repos/AppImage/type2-runtime/releases/latest \
        | jq -r '.assets[] | select(.name == "runtime-x86_64") | .browser_download_url')
    if [[ -z "$RUNTIME_URL" ]]; then
        echo "error: could not find an x86_64 runtime asset in the latest GitHub release" >&2
        exit 1
    fi
    curl -fsSL -o "$RUNTIME_FILE" "$RUNTIME_URL"
fi

# --- 2. Assemble the AppDir ---

echo "Assembling AppDir at $APPDIR ..."
rm -rf "$APPDIR"
mkdir -p \
    "$APPDIR/usr/bin" \
    "$APPDIR/usr/lib" \
    "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
    "$APPDIR/usr/share/opendrop/presets"

cp "$BINARY" "$APPDIR/usr/bin/opendrop-app"

# Bundle only the 4 libs from the policy above, resolving each SONAME to
# the real file ldd reports (following the distro's dev symlink) so the
# versioned file travels into the AppImage, then keeping a symlink under
# the SONAME the binary actually needs at load time.
BUNDLE_LIBS=(
    libndi.so.6
    libprojectM-4.so.4
    libavahi-client.so.3
    libavahi-common.so.3
)
for soname in "${BUNDLE_LIBS[@]}"; do
    lib_path=$(ldd "$BINARY" | awk -v s="$soname" '$1 == s && $2 == "=>" { print $3 }')
    if [[ -z "$lib_path" ]]; then
        echo "error: '$soname' not found in 'ldd $BINARY' output" >&2
        exit 1
    fi
    real_path=$(realpath "$lib_path")
    cp -L "$real_path" "$APPDIR/usr/lib/"
    real_name=$(basename "$real_path")
    if [[ "$real_name" != "$soname" ]]; then
        ln -sf "$real_name" "$APPDIR/usr/lib/$soname"
    fi
done

cp "$SCRIPT_DIR/opendrop-native.desktop" "$APPDIR/usr/share/applications/opendrop-native.desktop"
cp "$SCRIPT_DIR/icon-256.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/opendrop-native.png"

# AppImage spec requires the desktop file and the icon (named after the
# desktop file's Icon= value) at the AppDir root, plus .DirIcon; appimagetool
# refuses to build without them.
ln -sf usr/share/applications/opendrop-native.desktop "$APPDIR/opendrop-native.desktop"
ln -sf usr/share/icons/hicolor/256x256/apps/opendrop-native.png "$APPDIR/opendrop-native.png"
ln -sf opendrop-native.png "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
# $APPDIR is set by the AppImage runtime itself before AppRun executes;
# it must not be recomputed here.
export LD_LIBRARY_PATH="$APPDIR/usr/lib:$LD_LIBRARY_PATH"
exec "$APPDIR/usr/bin/opendrop-app" "$@"
EOF
chmod +x "$APPDIR/AppRun"

echo "Copying presets from $PRESETS_SRC ..."
rsync -a --exclude='.git' "$PRESETS_SRC/" "$APPDIR/usr/share/opendrop/presets/"

cp "$REPO_ROOT/LICENSE" "$APPDIR/usr/share/opendrop/LICENSE"
cp "$REPO_ROOT/app/assets/fonts/Inter-OFL.txt" "$APPDIR/usr/share/opendrop/Inter-OFL.txt"
cp "$REPO_ROOT/app/assets/fonts/JetBrainsMono-OFL.txt" "$APPDIR/usr/share/opendrop/JetBrainsMono-OFL.txt"

# --- 3. Build the AppImage ---

echo "Running appimagetool ..."
BUILD_LOG="$(mktemp)"
trap 'rm -f "$BUILD_LOG"' EXIT

if ! ARCH=x86_64 "$APPIMAGETOOL" --runtime-file "$RUNTIME_FILE" "$APPDIR" "$OUTPUT_PATH" >"$BUILD_LOG" 2>&1; then
    if grep -qi fuse "$BUILD_LOG"; then
        echo "appimagetool could not use FUSE directly, retrying with --appimage-extract-and-run ..." >&2
        ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run --runtime-file "$RUNTIME_FILE" "$APPDIR" "$OUTPUT_PATH"
    else
        cat "$BUILD_LOG" >&2
        exit 1
    fi
else
    cat "$BUILD_LOG"
fi

echo "Built $OUTPUT_PATH"
