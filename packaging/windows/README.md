# Windows packaging notes

## vcpkg projectM pin (4.1.6)

The Windows build resolves projectM through vcpkg, installed in classic
mode (no `vcpkg.json` manifest at the workspace root). A plain
`vcpkg install projectm:x64-windows` installs whatever is HEAD in the
local vcpkg git clone's ports tree at install time, not a fixed version.
That HEAD version drifts: it was 4.1.7 when this pin was introduced,
diverging silently from the version this project actually requires.

This project requires projectM **4.1.6** specifically, to match:

- The Linux build's real dependency: Arch's `libprojectm` system package
  (see `.planning/PHASE0-DECISION.md`).
- The LGPL third-party attribution in `app/src/ui/about.rs`, which names
  and links to the projectM v4.1.6 release tag.
- `engine/src/lib.rs`'s `version_is_4_1_6` test, which asserts against
  4.1.6 specifically.

To pin the installed version, `packaging/windows/overlay-ports/projectm`
vendors the vcpkg port files from the upstream commit that introduced the
4.1.6 port (superseded later by a 4.1.7 bump). Install via that overlay
port, do **not** run plain `vcpkg install projectm:x64-windows`, since
that reintroduces the same divergence this pin fixes.

### Recipe (run on the Windows build machine)

```
vcpkg remove projectm:x64-windows --recurse
vcpkg install projectm:x64-windows --overlay-ports=<repo>/packaging/windows/overlay-ports
cargo clean -p opendrop-engine
cargo test --workspace
```

`engine/build.rs`'s vcpkg discovery itself is unchanged by this: the
`--overlay-ports` flag only affects how the package gets installed, not
how the already-installed package is found by
`vcpkg::Config::find_package`.

See commit `30ff1ab` ("build(windows): pin vcpkg projectm to 4.1.6 via
overlay port") for the full history of why this pin exists.
