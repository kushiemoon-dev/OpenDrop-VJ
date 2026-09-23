# AUR packaging notes

`opendrop-vj-bin` downloads the Linux release AppImage and installs its
contents directly, rather than building from source. Building from source
would need the same NDI SDK download and pinned-projectM build the release
CI does, both a much heavier ask for an AUR user than for CI, so this
package skips it entirely: `--appimage-extract` unpacks the AppImage's
embedded squashfs (no FUSE needed, since it never mounts anything), and
`package()` installs that straight into `/opt/opendrop-vj`, with a thin
`/usr/bin/opendrop-vj` wrapper standing in for `AppRun` (setting `$APPDIR`
and `$LD_LIBRARY_PATH` itself, since there is no AppImage runtime here to
set `$APPDIR` before it runs).

`depends` lists every non-bundled `.so` the binary links against (checked
with `ldd` against a real build, not guessed): `packaging/appimage/
build-appimage.sh`'s `BUNDLE_LIBS` already ships `libndi`/`libprojectM-4`/
`libavahi-*` inside the AppImage itself, so those aren't system deps here;
everything else linked (pipewire, alsa, openssl, zlib, brotli, zstd,
libglvnd, dbus, systemd-libs, gcc-libs) is.

## Testing a change locally

```sh
cd packaging/aur
makepkg -f
```

Rebuilds and packages without installing. `pacman -Qlp *.pkg.tar.zst` to
inspect contents, `pacman -U *.pkg.tar.zst` to actually install it.

## Updating for a new release

1. Bump `pkgver` (and reset `pkgrel` to 1) in `PKGBUILD`.
2. Recompute the checksum: `sha256sum` the new release's
   `OpenDrop-Native-<version>-x86_64.AppImage` and update `sha256sums`.
3. `makepkg --printsrcinfo > .SRCINFO`.
4. Push both files to the AUR git repo (`ssh://aur@aur.archlinux.org/
   opendrop-vj-bin.git`) - needs an AUR account with this machine's SSH
   key registered to it; that account setup is a one-time manual step,
   not part of this repo.
