# Changelog

All notable changes to OpenDrop will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-22

First stable release — the 4-deck mixer, GPU compositor, automation (snapshots/timeline/LFO),
preset/video libraries, control surfaces (MIDI/OSC/Ableton Link/remote), and pro outputs
(NDI/Spout/virtual-cam/OBS) described in the README are all in place and exercised end to end.

### Added

- Video loop composited as a real WebGL texture layer inside the GPU compositor (5th layer,
  uploaded via `texImage2D` from the same rAF loop as the 4 deck canvases), replacing the old DOM
  `<video>` + CSS `mix-blend-mode:screen` approach — unreliable across two independently
  GPU-composited surfaces on some Chromium/Mesa/Wayland stacks, where the video stayed invisible
  regardless of blend mode. Decks composite among themselves first, unchanged; the video layer
  draws last, on top, at its own opacity — visible at exactly its own slider strength regardless of
  deck opacity/crossfader position.

### Fixed

- Render loop could die permanently: a CDN-hosted video clip is cross-origin, and the `<video>`
  element had no `crossorigin` attribute, so `texImage2D` threw an uncaught `SecurityError` that
  broke the `requestAnimationFrame` chain — nothing rendered again until reload. Fixed the
  attribute and wrapped the render tick in try/catch so no future per-frame error can freeze the
  whole app again either.
- `sendVideo` threw `DataCloneError` on `BroadcastChannel.postMessage` — a raw Svelte 5
  `$state`-proxied clip object isn't structured-cloneable; same class of bug already handled for
  `sendOverlays`/`sendQVars`/`sendPoll`, `sendVideo` was the one missed.
- Video clip list rows collapsed to ~2px tall instead of scrolling normally once there were enough
  clips to overflow the list's `max-height` — `overflow:hidden` (needed for name ellipsis) disables
  flexbox's automatic minimum-size protection; `flex-shrink:0` restores it.

## [0.9.0] - 2026-07-20

### Added

- Per-deck NDI sender management (main process), per-slot canvas readback, and toggle UI
- OBS WebSocket connection management (main process), scene/slot/mood mapping, bidirectional scene link with anti-echo guard, and mood-label config UI
- Twitch and Kick chat connections (main process), chat-poll vote parsing/tally/resolution, lifecycle wiring, overlay UI, and relay to the output window
- `safeStorage`-backed secrets store for streaming credentials, with dedicated secret input fields for OBS/Twitch/Kick (configurable poll source, auto-dismiss)

### Fixed

- CDN video loops never reactive in the UI; added clip selection for auto-cut rotation
- Renderer-side OBS listener leak
- OBS host/port/scene-mapping now persisted across restarts
- `startNdiDeck` guarded against re-entrant calls for the same slot
- `deckframe:post` guarded against synchronous NDI send errors
- OBS scene-change listener now registered once; disconnect errors guarded
- One-shot anti-echo flag replaced with direct scene-name comparison

### Security

- Pinned `axios` to `>=1.18.0` via pnpm override (transitive dependency of `@retconned/kick-js`, `wait-on`, `audify`) — resolves 3 moderate DoS advisories

## [0.8.0] - 2026-07-16

### Added

- Live video input as a deck source: webcam and NDI network sources (Electron)
- Runtime `.milk`/`.prjm` MilkDrop preset import via drag-and-drop — no rebuild needed

### Fixed

- Linux output-window audio reactivity: PCM capture retry decoupled from the one-shot state resync (previously required re-picking the audio device once per session)

### Changed

- `+page.svelte` (2,570+ lines) further split — 15 new engine stores and action modules extracted (deck/mixer state, beat-sync, MIDI connection/mapping, audio source, Electron feature toggles, run status, share-set, the visualizer startup sequence, output-window management); down to ~1,900 lines

## [0.7.0] - 2026-07-11

### Added

- Reliable, self-hosted star-history badge (replaces the flaky third-party service)

### Fixed

- `od-pl-mode` read from localStorage is now validated instead of blindly cast
- No timer scheduled when the playlist interval is `Infinity`

### Changed

- `+page.svelte` (1,500+ lines) split into focused engine modules and stores — cloud presets, overlay, video-loop playback, and playlist subsystems extracted, plus 14 sidebar sections (stage-common and mixer-only) pulled into standalone components; each extraction preceded by characterization/reactive-wrapper tests
- README rewritten for the current 4-deck feature set

## [0.4.1] - 2026-06-15

### Added

- All 1754 Milkdrop presets now available (up from 100) via lazy-loading — names appear instantly, data loads on first use

### Fixed

- Linux loopback button no longer opens the Wayland screen-share portal; shows PipeWire setup instructions instead
- Preset browser: removed author tag chips (unworkable at 1754 presets), kept ★ favorites filter and search
- Preset list items more compact to show more presets at once

## [0.4.0] - 2026-06-14

> **Complete rewrite**: web-first stack (SvelteKit + Butterchurn + Electron)
> replacing Tauri / Rust / projectM C++ entirely. Zero native toolchain —
> runs on Windows, Linux, and Mac out of the box.

### Added

- **Butterchurn** visualizer engine (Milkdrop WebGL, 100 presets via `butterchurn-presets`)
- **2 independent decks A/B** — separate Butterchurn instances on their own canvases
- **Crossfader** A↔B via opacity (0–1 slider, arrow-key shortcuts ±0.05)
- **Per-deck playlists**: sequential / shuffle auto-cycle, 2–120 s interval, add/remove/prev/next
- **Beat-sync / BPM**: `BeatDetector` — bass-energy detection, 43-frame history, 300 ms cooldown
- **MIDI mapping** (Web MIDI API, Chromium/Electron only): CC/notes → crossfader, playlists
- **Favorites** ★ persisted in localStorage
- **Author-derived tag filters** with chip UI
- Live **preset search**
- Detached **output window** at `/output` — designed for OBS Browser Source
- Audio sources: mic, device picker, audio file, `getDisplayMedia` (display capture)
- **System audio loopback** via Electron `desktopCapturer` (Linux/Windows)
- **Export / import** playlists as JSON (`opendrop-playlists.json`)
- **Electron shell**: `electron/main.cjs` + `electron/preload.cjs` (contextIsolation, BroadcastChannel IPC relay, `app://` protocol for packaged builds)
- PipeWire monitor setup script: `scripts/setup-audio.sh` (Linux)
- Vitest unit tests for midi + playlist engines; 15 Playwright E2E tests

### Changed

- Dropped Tauri / Rust / projectM C++ / OpenGL renderer sidecar / v4l2loopback / Spout / NDI
- App is a SvelteKit SPA (`adapter-static`, `ssr: false`) — Electron wraps it as a native desktop app
- Bumped concurrently to v10, electron-builder to v26 (security fixes)
- CI: pnpm/action-setup v4, softprops/action-gh-release v2, Node.js 22 on all runners
- pnpm v11: overrides and allowBuilds moved to `pnpm-workspace.yaml`

### Known Limitations

- Blend modes not implemented — opacity crossfade only
- 100 / 1754 presets loaded (full library load deferred)
- Web MIDI: Chromium / Electron only (Firefox / Safari unsupported)
- System audio loopback: Electron only (unavailable in plain browser)
- Installers unsigned (code signing deferred)

## [0.2.0] - 2026-01-22

### Added

- 235 frontend tests with Vitest (87.57% coverage)
- Toast notification system for user feedback
- Favorites system for presets (star button + localStorage)
- Categories auto-detection from preset paths
- Manual tags system for presets
- Import presets from folder command
- Export/Import playlists (JSON format)
- Theme toggle (dark/light mode)
- Accent color picker (6 presets: cyan, magenta, purple, green, orange, yellow)
- Lucide icons across all components
- CSS animations and transitions
- Windows monitor detection via Win32 API
- macOS monitor detection via CoreGraphics
- GitHub Actions CI/CD for automated builds (resolves #1)

### Fixed

- MIDI port name now tracked in MidiController
- Audio pump errors show toast after 5 consecutive failures
- Preset path validation (exists, is file, valid extension)
- Renderer Ready state properly updated from stdout events
- Playlist shuffle fallback for edge cases

### Changed

- Replaced inline SVGs with Lucide icons (8 components)
- Console.error replaced with toast notifications
- Improved responsive layout with collapsible sidebar

## [0.1.0-alpha] - 2026-01-21

### Added

- Initial release
- 4-deck audio visualizer with ProjectM
- MIDI controller support with learn mode
- Video output via v4l2loopback/Spout/NDI
- Preset browser with search and filtering
- Playlist management with shuffle and auto-cycle
- Crossfader with multiple blend modes
