# Changelog

All notable changes to OpenDrop will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.1] - 2026-06-14

### Fixed
- CI: pnpm-workspace.yaml missing `packages` field; `allowBuilds` had placeholder values
- CI: update `pnpm/action-setup` to v4 and `softprops/action-gh-release` to v2 (Node.js 20 deprecated June 16)
- CI: bump Node.js to 22 on runners
- Security: bump concurrently to v10 (fixes shell-quote critical CVE)
- Security: bump electron-builder to v26 (fixes tar high CVEs)
- Security: add pnpm overrides for shell-quote, tar, esbuild, cookie transitive deps
- pnpm: move build allowlist from `onlyBuiltDependencies` (deprecated) to `pnpm-workspace.yaml` `allowBuilds` (pnpm v11)

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
