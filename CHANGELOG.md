# Changelog

## v0.1.4 (2026-09-23)

### New features
- Bundled the Ansorre preset pack directly in packaged releases
- Added an `opendrop-vj-bin` AUR PKGBUILD for Arch Linux users
- Added a Quick start section to the docs for people running a release build

### Fixes
- Presets now merge into a separate directory instead of the cached one, avoiding a stale-cache collision

## v0.1.3 (2026-09-23)

### New features
- macOS packaging: App bundle, a pinned projectM 4.1.6 build, and a dedicated CI release job (published as an experimental download)
- SEO metadata and optimized landing page assets
- Fetch-only Cloud Video panel, pointed at the project's own CDN by default

### Fixes
- Several macOS build issues chained from the new packaging job: dylibs resolved by prefix instead of an assumed unversioned filename, real dylib paths resolved instead of otool's `@rpath` install names, `opengl.pc` stubbed so pkg-config finds projectM-4, and submodules recursed when building projectM 4.1.6 from source
- Star history refresh workflow now opens a PR instead of pushing directly to the protected main branch

## v0.1.2 (2026-09-06)

### New features
- Per-deck video pipeline: state, decode plumbing, beat-cut and audio-warp driven from the tick loop, and compositor input swapped to the decode texture in video mode
- Auto-load a matching clip onto a deck from Rekordbox Link
- NDI source discovery now only runs while the NDI-in panel is open

### Fixes
- Mobile and tablet layout repaired on the landing page

## v0.1.1 (2026-09-05)

### New features
- Real screenshots added to the landing page gallery and README

### Fixes
- WASAPI loopback now captures the actual output device for Windows system audio
- Bundle the official VC++ Redistributable instead of the Visual Studio toolset's copy, and dropped the now-redundant install step

## v0.1.0 (2026-09-04)

Initial release of the native engine rewrite: a from-scratch Rust implementation (audio, core, engine, io, NDI) replacing the previous Electron/web app. The version numbering restarts here because this is a new codebase under the same repository; the prior app's history and tags (v0.2.0 through v1.0.1 below) belong to that earlier project. See the anomalies note at the end of this file for how the two histories relate.

## v1.0.1 (2026-08-27)

### New features
- GitHub Actions CI workflow for check, lint, test, and e2e
- README documentation for the OBS/Twitch/Kick streaming integration

### Fixes
- `app://` protocol handler now guards against path traversal
- Fixed pre-existing type-check and lint issues surfaced by the new CI job
- E2E assertions updated for the 4-deck compositor refactor

### Internal
- Dependency bump for brace-expansion, fast-uri, js-yaml, postcss, and sharp (osv-scanner CVE fix)
- GitHub Actions pinned to commit SHA
- Added nvmrc/engines and eslint/prettier tooling
- Removed the dead `sampleCanvasRGBA` export (flagged by knip)

## v1.0.0 (2026-07-22)

First stable release.

### New features
- Compositing the video loop as a WebGL texture layer

### Fixes
- Video clip list rows no longer flex-shrink to an unreadable height
- Clip data is shallow-copied before `postMessage` to avoid a `DataCloneError`

## v0.9.0 (2026-07-20)

### New features
- OBS WebSocket integration: bidirectional scene link with an anti-echo guard, scene/slot/mood mapping, and setup UI
- Twitch and Kick chat-poll: connection, vote parsing/tally, overlay display, and credential setup
- Per-deck NDI sender management with a toggle UI
- `safeStorage`-backed secrets store for streaming credentials

### Fixes
- CDN video loops were never reactive in the UI, added clip selection for auto-cut rotation
- OBS host/port/scene-mapping now persists across restarts

## v0.8.0 (2026-07-16)

### New features
- Live camera/NDI video inputs and `.milk` preset drag-and-drop import
- CI now deploys over SSH on push to main, triggers only on version tags, and skips native addon builds for the static web build

### Internal
- Large refactor moving orchestration logic (fullscreen/canvas-resize, visualizer startup, share-set, preset-loading/beat-tempo, MIDI connection, Electron toggle/audio-source, beat-sync, deck/mixer state) out of `+page.svelte` into dedicated stores

## v0.7.0 (2026-07-11)

Continuation of the v0.6.0 milestone series, developed on a branch and merged back rather than built directly on top of the v0.6.0 tag; see the anomalies note for the history detail.

### New features
- Self-hosted star history with a shields badge

### Internal
- Completed the store-extraction refactor: playlist, video-loop playback, overlay, and cloud-presets subsystems each extracted from `+page.svelte` into a store, with tests written first
- Dead code removed (flagged by knip)
- French UI strings and comments translated to English
- README rewritten for the current 4-deck feature set

## v0.6.0 (2026-07-06)

Large feature release, the M1 through M9 milestone series.

### New features
- Per-slot compositing system: WebGL compositor wired into the stage and output windows, per-slot blend logic, color controls per deck via CSS filter
- Text overlay creation and rendering, overlay queue with auto-cycling
- Snapshot capture/recall and a time-param injection engine, both with dedicated UI panels
- Per-deck beat/volume trigger controls, MIDI LED feedback
- Timeline keyframe engine with UI and playback wiring
- Share-set encode/decode with a shareable link and import confirmation
- Q-var injection engine, 128 command-slot stubs, and a watchlist UI panel
- Cloud presets Worker (CRUD over R2, quota-enforced) and a "Mes presets" UI panel
- Remappable keyboard bindings, a central `CommandRegistry` replacing the old MIDI action switch, unified `Clock`/`LfoEngine` for tempo and modulation
- MIDI enrichment: multi-controller identity, pitchbend, 14-bit CC, soft-takeover, MIDI clock IN
- OSC control and a web remote UI for phone/tablet, screen targeting and direct fullscreen
- Ableton Link sync, 4-bus independent output via `DeckManager`
- Bundled video-loop clips and a CDN video-loops pipeline, live at loops.kushie.dev

### Fixes
- Dependency bump for undici (multiple open Dependabot advisories)
- Svelte state-proxy leak into the global Q-var object

## v0.5.3 (2026-06-20)

CPU performance optimization.

### New features
- Per-deck FPS cap and eco-throttle for invisible or paused decks

### Fixes
- Displayed FPS now reflects actual Butterchurn render calls instead of always showing the screen refresh rate

## v0.5.2 (2026-06-19)

4-deck compositor and a Spout Windows output skeleton.

### New features
- Vendored SpoutDX sources, stream controls moved to the mixer panel
- Virtual webcam output via `v4l2loopback` and an ffmpeg pipe
- Output window made audio-reactive via PCM streaming (autoplay-policy switch plus a worklet-based capture path)

### Fixes
- README clarifies the desktop app limitation: the web version covers Windows/macOS
- Svelte 5 dependency tracking fixed for crossfader/overlays/video sync to the output window: state was read through optional chaining, which short-circuited argument evaluation and left the effect never re-running

## v0.5.0 (2026-06-18)

### New features
- 4-slot mixer layout with an equal-power crossfader and bus-to-output projection
- Virtualized preset grid with lazy WebP thumbnail generation, via a single offscreen Butterchurn renderer
- Video-loop build pipeline (download, transcode, manifest) with bundled and optional CDN delivery
- Global CSS design tokens, `DeckManager` for lazy 4-slot management

## v0.4.8 (2026-06-17)

Music-reactive video loop background layer.

## v0.4.7 (2026-06-17)

Replaced `naudiodon-loopback` with `audify` (an RtAudio wrapper) for WASAPI loopback capture. The previous module's segfault-handler dependency used NAN, which is incompatible with Electron 42's V8 and never compiled in CI, so the Outputs section silently never appeared in the device picker.

## v0.4.6 (2026-06-17)

Per-device output loopback with a 2-section audio picker, letting users pick any output device instead of only the system default, after several CI build-script approval fixes for the native `naudiodon` module in the pnpm workspace.

## v0.4.5 (2026-06-17)

Fixed audio device enumeration in Electron: without `setPermissionRequestHandler`, `getUserMedia` was silently denied, so `enumerateDevices` returned only the default microphone, unlabeled or missing other inputs, which also blocked BPM detection on non-default sources.

## v0.4.4 (2026-06-17)

### Fixes
- Megapack presets (16376 files, 102 MB) are now downloaded from the `megapack-v1` release during CI before build, since only the 1754 bundled presets were previously available to CI builds
- Windows loopback audio: `getDisplayMedia` requires both an audio and a video stream to be fulfilled, fixed by always fetching a screen source alongside the loopback audio and stopping the video track right after capture

## v0.4.3 (2026-06-17)

Visual quality overhaul: render fidelity, megapack presets, overlays, and NDI output. Same commit as the `megapack-v1` pre-release, which packages the preset archive as a downloadable asset rather than an app version, see the anomalies note.

### Fixes
- Robust `app://` protocol handler: `registerSchemesAsPrivileged` was never called and `net.fetch` did not guarantee correct MIME types for local files, both causing a blank window on Windows
- `electron-builder --publish never`, letting a single `create-release` job assemble the GitHub release instead of racing with per-platform uploads that produced orphaned drafts
- NDI (`grandiose`) build decoupled from `electron-builder`'s rebuild step and moved to `optionalDependencies`, since the NDI SDK isn't available on GitHub-hosted Linux/macOS runners
- `file://` URLs malformed on Windows: replaced string concatenation with `pathToFileURL()` to handle drive letters and backslashes correctly

## v0.4.2 (2026-06-15)

Unified cross-platform system audio capture and web deployment.

### Fixes
- Output window no longer stayed black on web

## v0.4.1 (2026-06-15)

All 1754 presets and a Linux loopback fix.

### New features
- All 1754 bundled presets now load lazily via `import.meta.glob` instead of the previous 100-preset subset

### Fixes
- Loopback button on Linux now shows a PipeWire setup hint instead of opening the Wayland screen-share portal

## v0.4.0 (2026-06-14)

Complete web-first rewrite of the app.

### Internal
- CI pipeline and security fixes
- pnpm overrides migrated from `package.json` to `pnpm-workspace.yaml`, since pnpm v11 no longer reads `pnpm.overrides` from `package.json`

## v0.3.6 (2026-04-15)

Windows compatibility fixes and an in-app error log.

### New features
- Error log ring buffer with Tauri commands, surfaced in the settings panel with copy-to-clipboard for bug reports

### Fixes
- Graceful GL setup error handling instead of panicking, forwarded to the UI
- Audio thread join now has a 2s timeout, `try_lock` used to prevent a UI freeze
- No-audio-detected hint added for loopback devices

## v0.3.5 (2026-03-16)

Windows compatibility and renderer stability, plus preset browser improvements.

## v0.3.4 (2026-01-26)

### Fixes
- Duplicate paths in the preset browser fixed, browser made responsive

### Internal
- Added a Gitea sync CI workflow

## v0.3.3 (2026-01-25)

Windows bug fixes and UI improvements, including a new texture and preset patch fixes merged from a community pull request.

## v0.3.1 (2026-01-24)

Fixed a Windows renderer crash and audio issues. No v0.3.2 tag exists, see the anomalies note.

## v0.3.0 (2026-01-24)

Bundled presets, Windows audio improvements, cross-platform fixes, and a settings panel.

## v0.2.2 (2026-01-23)

Fixed Windows static linking for projectM.

## v0.2.1 (2026-01-22)

Dark theme set as default, header layout and projectM linking fixes.

## v0.2.0 (2026-01-22)

Initial tagged release: GitHub Actions CI/CD release workflow, cross-platform projectM build configuration, and UI/UX improvements with stores and tests, following an earlier untagged alpha commit.

---

### Notes on tag history

- **v0.1.0 through v0.1.4** (native engine rewrite) and **v0.2.0 through v1.0.1** (previous Electron/web app) are two disjoint commit histories with no common ancestor, both tagged in this same repository. The native rewrite's version numbering restarts from 0.1.0 rather than continuing from v1.0.1.
- **v0.3.2** was never tagged. The tag sequence goes directly from v0.3.1 to v0.3.3.
- **`megapack-v1`** and **v0.4.3** point to the same commit. `megapack-v1` is a pre-release asset (the preset pack archive), not an app version.
- **v0.6.0 to v0.7.0** and **v1.0.0 to v1.0.1** are not fast-forward: both pairs share a real common ancestor further back, reached via a branch merge rather than direct linear history.
