# macOS packaging notes

## projectM 4.1.6 pin

Same requirement as `packaging/windows/README.md`: this project requires
projectM **4.1.6** specifically (`app/src/ui/about.rs`'s LGPL attribution,
`engine/src/lib.rs`'s `version_is_4_1_6` test). Homebrew's `projectm`
formula tracks upstream HEAD, not a fixed version - the same drift problem
Windows solved with a pinned vcpkg overlay port.

There is no equivalent of a pinned Homebrew formula here, so the release
build compiles projectM 4.1.6 from source instead:

`--recurse-submodules` is required: `vendor/projectm-eval` is a git
submodule, and CMake's `add_subdirectory` fails on an uninitialized one
with no clearer error than a missing `CMakeLists.txt` (found by running
this for real in CI).

```
git clone --recurse-submodules --branch v4.1.6 --depth 1 https://github.com/projectM-visualizer/projectm /tmp/projectm-src
cmake -S /tmp/projectm-src -B /tmp/projectm-build \
    -DCMAKE_INSTALL_PREFIX=/tmp/projectm-4.1.6-install \
    -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/projectm-build --parallel
cmake --install /tmp/projectm-build
export PKG_CONFIG_PATH="/tmp/projectm-4.1.6-install/lib/pkgconfig"
```

`projectM-4.pc` declares `Requires: opengl`, which has no macOS
equivalent (OpenGL is a system framework there, not a pkg-config
package - `engine/build.rs`'s macOS branch links `-framework OpenGL`
directly). The release job writes an empty stub `opengl.pc` next to it
after install to satisfy that dependency; without it `pkg-config --libs
--cflags projectM-4` fails with "Package opengl was not found".

`engine/build.rs`'s macOS branch discovers projectM via `pkg_config`, same
as the Linux branch - it just needs `PKG_CONFIG_PATH` pointed at this
from-source install instead of a system package.

## OpenGL linking

macOS links system frameworks with `-framework OpenGL`, not
`-l dylib=OpenGL` like Linux - `engine/build.rs` branches on
`target_os = "macos"` for this, it is not covered by the generic
"not Windows" case the Linux branch used to be.

## NDI SDK

Installed from the official `.pkg`
(`https://downloads.ndi.tv/SDK/NDI_SDK_Mac/Install_NDI_SDK_v6_Apple.pkg`),
same silent-install-with-Gatekeeper-exclusion approach as the Windows CI
step for the `.exe` installer.

## Unsigned build - no Apple Developer ID

This build has no code signature or notarization ticket (no paid Apple
Developer account behind it). Gatekeeper quarantines the downloaded `.app`
on first launch:

> "OpenDrop-Native.app" cannot be opened because the developer cannot be
> verified.

To run it anyway: right-click (or Control-click) the app in Finder → Open
→ confirm in the dialog that appears. This only needs doing once per
download. Command-line equivalent:

```
xattr -dr com.apple.quarantine /path/to/OpenDrop-Native.app
```

## Architecture

`packaging/macos/build-app.sh` targets `arm64` only (GitHub's `macos-latest`
runner is Apple Silicon). No Intel Mac build is produced.
