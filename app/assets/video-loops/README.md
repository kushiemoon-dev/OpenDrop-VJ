# Video Loops

Drop-in folder for the **bundled** video loops the Video panel offers as
built-in clips (marked 📦 in the panel, and not deletable from inside the
app).

This directory ships **empty on purpose**. The OpenDrop-VJ web app served
~46 `.webm` loops from a public CDN (`cdn-video-loops/` + its
`manifest.json`); relocating those files into this repository is an asset
task, not a code task, and is deliberately left as a manual step: the same
treatment the "megapack Ansorre" preset pack gets (Phase 8 plan, Override
5). An empty or missing folder is a normal, supported state: the Video
panel simply shows only the user's own clips.

## Where the app looks

The panel scans two directories, bundled first, then the user's own:

| Kind | Location | Managed by |
| --- | --- | --- |
| Bundled (📦) | `<directory of the opendrop-app binary>/assets/video-loops/` | you, by hand (or a packaging step) |
| User | `<data dir>/opendrop-native/video-clips/` | the panel's **+ Video** button |

`<data dir>` is `directories::ProjectDirs::data_dir()`: on Linux
`~/.local/share/opendrop-native/`, on macOS
`~/Library/Application Support/opendrop-native/`, on Windows
`%APPDATA%\opendrop-native\data\`. It is the **data** directory, not the
config directory that holds `ui.json`.

Each directory is scanned one level deep (no recursion) and sorted by
filename, so the clip order is stable across runs. A clip's display name is
its filename without the extension.

## Adding loops

Either:

- copy files next to the built binary, under `assets/video-loops/`, to have
  them appear as bundled clips: this is the path a packaging script should
  populate, alongside the fonts it already copies (see
  `packaging/appimage/build-appimage.sh`); or
- use the Video panel's **+ Video** button, which copies whatever you pick
  into the user clip folder above (files over 50 MB are refused, matching
  the web app's own import cap).

## Accepted formats

`.webm`, `.mp4`, `.mov`, `.mkv`, `.avi`, `.m4v`, `.ogv`, `.mpg`.

The extension list is only a filter for the folder scan: decoding is done
by `ffmpeg`, so anything your `ffmpeg` build can open will play. **`ffmpeg`
must be on `PATH`**: it is the same runtime dependency the v4l2loopback
output path already has.

## Practical notes

- Loops play seamlessly (`-stream_loop -1`), so short clips are fine and
  preferable.
- Every clip is decoded to 1280x720 RGBA regardless of its own resolution;
  there is no benefit to shipping anything larger.
- Keep clips short and small: the beat-driven auto-cut restarts the decoder
  on every cut, and a multi-hundred-megabyte file makes that restart
  visible.
