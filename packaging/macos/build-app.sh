#!/usr/bin/env bash
# Assembles the .app bundle and zips it for OpenDrop-Native's macOS release.
#
# Usage: packaging/macos/build-app.sh
# Prerequisite: `cargo build --release` (this script does not build Rust).
#
# Output: OpenDrop-Native-<version>-macos-arm64.zip at the repo root, where
# <version> is read from app/Cargo.toml's [package].version.
#
# Shared-library bundling policy (mirrors packaging/appimage/build-appimage.sh's
# Linux policy - same two libraries, no macOS system package provides either):
#   Bundled into Contents/Frameworks/, with the binary's load commands
#   rewritten via install_name_tool to @executable_path/../Frameworks/...:
#     - libndi.dylib        (NDI SDK has no Homebrew package)
#     - libprojectM-4.dylib (built from source at the pinned 4.1.6, see
#       README.md in this directory - Homebrew's projectm formula floats)
#   Never bundled: anything under /System or /usr/lib (Metal/OpenGL/
#   CoreAudio stack) - same driver-coupling reasoning as the Linux script's
#   libGLESv2/libGLdispatch exclusion.
#
# Unsigned build: no Apple Developer ID here (see README.md in this
# directory), so Gatekeeper quarantines the app on first launch. Documented
# for end users in the same README rather than worked around here.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

BINARY="$REPO_ROOT/target/release/opendrop-app"
APP_NAME="OpenDrop-Native.app"
APP_DIR="$SCRIPT_DIR/$APP_NAME"
CARGO_TOML="$REPO_ROOT/app/Cargo.toml"
# Same override convention as the Linux script's PRESETS_SRC.
PRESETS_SRC="${PRESETS_SRC:-/srv/http/opendrop-presets}"

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

OUTPUT_PATH="$REPO_ROOT/OpenDrop-Native-${VERSION}-macos-arm64.zip"

# --- 1. Assemble the .app bundle skeleton ---

echo "Assembling $APP_DIR ..."
rm -rf "$APP_DIR"
mkdir -p \
    "$APP_DIR/Contents/MacOS" \
    "$APP_DIR/Contents/Resources/presets" \
    "$APP_DIR/Contents/Frameworks"

cp "$BINARY" "$APP_DIR/Contents/MacOS/opendrop-app"

cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>OpenDrop-Native</string>
    <key>CFBundleDisplayName</key>
    <string>OpenDrop-Native</string>
    <key>CFBundleIdentifier</key>
    <string>dev.kushie.opendrop-native</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>opendrop-app</string>
    <key>CFBundleIconFile</key>
    <string>opendrop-native.icns</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSCameraUsageDescription</key>
    <string>OpenDrop-Native uses the camera for the Video panel's webcam input.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>OpenDrop-Native uses the microphone for audio-reactive visuals.</string>
</dict>
</plist>
PLIST

# --- 2. Icon: .icns generated from the same source PNG the AppImage uses ---

ICONSET="$SCRIPT_DIR/.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET/opendrop-native.iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$REPO_ROOT/packaging/appimage/icon-256.png" \
        --out "$ICONSET/opendrop-native.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$REPO_ROOT/packaging/appimage/icon-256.png" \
        --out "$ICONSET/opendrop-native.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET/opendrop-native.iconset" -o "$APP_DIR/Contents/Resources/opendrop-native.icns"
rm -rf "$ICONSET"

# --- 3. Bundle the two non-system dylibs (policy documented above) ---
#
# The binary references both by install name (`otool -L`), not by a real
# filesystem path - grafton-ndi/the projectM build link them via @rpath,
# so the path `otool -L` prints can't be `cp`'d directly (found by running
# this for real in CI: "cp: .../@rpath/libndi.dylib: No such file or
# directory"). Resolve the real file separately, from where each SDK
# actually installs it.
resolve_dylib_source() {
    case "$1" in
        libndi.dylib)
            find "${NDI_SDK_DIR:?NDI_SDK_DIR must be set}" -name libndi.dylib -print -quit
            ;;
        libprojectM-4.dylib)
            find "${PROJECTM_INSTALL_PREFIX:?PROJECTM_INSTALL_PREFIX must be set}/lib" -name libprojectM-4.dylib -print -quit
            ;;
        *)
            echo "error: no source resolver defined for '$1'" >&2
            ;;
    esac
}

BUNDLE_LIBS=(
    libndi.dylib
    libprojectM-4.dylib
)
for soname in "${BUNDLE_LIBS[@]}"; do
    load_name=$(otool -L "$BINARY" | awk -v s="$soname" '$1 ~ s { print $1; exit }')
    if [[ -z "$load_name" ]]; then
        echo "error: '$soname' not found in 'otool -L $BINARY' output" >&2
        exit 1
    fi
    real_path=$(resolve_dylib_source "$soname")
    if [[ -z "$real_path" ]]; then
        echo "error: could not locate a real file for '$soname' on disk" >&2
        exit 1
    fi
    cp -L "$real_path" "$APP_DIR/Contents/Frameworks/$soname"
    install_name_tool -id "@executable_path/../Frameworks/$soname" "$APP_DIR/Contents/Frameworks/$soname"
    install_name_tool -change "$load_name" "@executable_path/../Frameworks/$soname" "$APP_DIR/Contents/MacOS/opendrop-app"
done

echo "Copying presets from $PRESETS_SRC ..."
rsync -a --exclude='.git' "$PRESETS_SRC/" "$APP_DIR/Contents/Resources/presets/"

cp "$REPO_ROOT/LICENSE" "$APP_DIR/Contents/Resources/LICENSE"
cp "$REPO_ROOT/app/assets/fonts/Inter-OFL.txt" "$APP_DIR/Contents/Resources/Inter-OFL.txt"
cp "$REPO_ROOT/app/assets/fonts/JetBrainsMono-OFL.txt" "$APP_DIR/Contents/Resources/JetBrainsMono-OFL.txt"

# --- 4. Zip the bundle (ditto, not `zip -r`, to preserve bundle metadata) ---

rm -f "$OUTPUT_PATH"
ditto -c -k --sequesterRsrc --keepParent "$APP_DIR" "$OUTPUT_PATH"

echo "Built $OUTPUT_PATH"
