# OpenDrop VJ

### Multi-Deck Milkdrop Visualizer — web-first, cross-platform

[![Live Demo](https://img.shields.io/badge/demo-opendrop.kushie.dev-ff2d78?style=flat-square)](https://opendrop.kushie.dev)
[![Release](https://img.shields.io/github/v/release/kushiemoon-dev/OpenDrop-VJ?style=flat-square)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![License](https://img.shields.io/github/license/kushiemoon-dev/OpenDrop-VJ?style=flat-square)](LICENSE)

**Open-source NestDrop alternative.** Real-time Milkdrop audio visualization with
2-deck mixing, MIDI control, and OBS Browser Source output — powered by
[Butterchurn](https://github.com/jberg/butterchurn) (WebGL, no C++ required).

> **v0.4.0** is a complete rewrite of the original Tauri/Rust/projectM v1
> (which never ran reliably on Windows or Linux). The new stack is web-first:
> **zero native toolchain** needed, runs identically on Windows, Linux, and Mac.

---

## Features

- **2 decks A/B** — independent Butterchurn (Milkdrop WebGL) instances
- **Crossfader** — opacity blend A↔B, arrow-key shortcuts (±0.05)
- **Playlists** — per-deck auto-cycle (sequential / shuffle), 2–120 s interval, prev / next
- **Beat-sync / BPM** — bass-energy beat detector drives preset changes on the beat
- **MIDI mapping** — CC/note → crossfader, playlist controls *(Chromium / Electron only)*
- **Preset browser** — search, favorites ★, 1754 presets (lazy-loaded)
- **Output window** — detached `/output` route for second monitor or OBS Browser Source
- **Audio sources** — mic, device picker, audio file, system audio (all platforms)
- **Export / import** playlists as JSON
- **Keyboard shortcuts** — ← → nudge crossfader, Tab switches active deck

---

## Stack

| Layer | Tech |
|-------|------|
| UI | SvelteKit + Svelte 5 runes, TypeScript |
| Visualizer | [Butterchurn](https://github.com/jberg/butterchurn) (Milkdrop WebGL) + [butterchurn-presets](https://github.com/jberg/butterchurn-presets) |
| Audio | Web Audio API (`AudioContext`, `AnalyserNode`) |
| MIDI | Web MIDI API |
| Build | Vite + `@sveltejs/adapter-static` (SPA, `ssr: false`) |
| Desktop | Electron 42 (optional — adds loopback audio + native window) |

---

## Try it

**→ [opendrop.kushie.dev](https://opendrop.kushie.dev)** — no install required (Chrome/Edge recommended).

## Quick Start (self-hosted)

```bash
pnpm install
pnpm dev          # → http://localhost:1420
```

Click **▶ Start** to initialize the audio context, then pick a preset in the browser panel.

## Desktop (Electron)

```bash
pnpm electron:dev     # Vite dev + Electron in parallel
pnpm electron:build   # Build SPA then package with electron-builder
```

**Linux / Hyprland (Wayland)** — pass flags when launching the packaged app:

```bash
./OpenDrop-VJ.AppImage --ozone-platform=wayland --no-sandbox
```

## System Audio Capture

Click **🔊 Audio système** in the app — behaviour adapts per platform:

| Platform | Web | Electron |
|----------|-----|----------|
| **Windows** | Screen picker → "Share system audio" (Chrome) | Native loopback — no picker |
| **Linux** | Device picker → `.monitor` source (PipeWire/Pulse) | Same |
| **macOS** | Tab audio only | Install [BlackHole](https://github.com/ExistentialAudio/BlackHole) → device picker |

**Linux optional:** `bash scripts/setup-audio.sh` creates a named PipeWire virtual source ("OpenDrop - Son du PC") instead of using a raw `.monitor` device.

## OBS Browser Source

1. Add a **Browser Source** in OBS
2. URL: `http://localhost:1420/output` (dev) or the packaged app URL
3. Width / Height: match your stream resolution

The `/output` route is a fullscreen crossfaded canvas — no UI chrome.

---

## Development

```bash
pnpm check          # svelte-check (TypeScript + Svelte)
pnpm test           # Vitest unit tests (midi engine, playlist engine)
pnpm test:coverage  # Coverage report
pnpm test:e2e       # Playwright E2E — 15 tests (requires pnpm dev running)
pnpm build          # Production SPA → build/
```

---

## Project Structure

```
src/
├── lib/
│   ├── engine/
│   │   ├── audio.ts      AudioEngine — AudioContext, source switching, AnalyserNode
│   │   ├── bpm.ts        BeatDetector — bass-energy beat detection
│   │   ├── deck.ts       Deck — Butterchurn instance wrapper
│   │   ├── midi.ts       MidiEngine — Web MIDI, CC/note mapping
│   │   ├── playlist.ts   PlaylistEngine — auto-cycle, shuffle, prev/next
│   │   └── sync.ts       Cross-window state sync (BroadcastChannel / Electron IPC)
│   └── presets/
│       └── index.ts      Preset registry (butterchurn-presets, search, categories)
└── routes/
    ├── +page.svelte      Main VJ controller UI
    └── output/
        └── +page.svelte  Fullscreen output canvas (OBS / second monitor)

electron/
├── main.cjs              Electron main process (IPC relay, loopback, app:// protocol)
└── preload.cjs           contextBridge → window.electronAPI

e2e/
└── app.spec.ts           Playwright E2E tests
```

---

## Known Limitations & Roadmap

| Item | Status |
|------|--------|
| Blend modes (Normal / Add / Screen…) | Not yet — opacity crossfade only |
| Web MIDI | Chromium / Electron only (not Firefox / Safari) |
| System audio on macOS browser | Tab audio only — install BlackHole for full capture |
| Signed installers | Planned |
| 4-deck compositor | Planned |
| NDI / Spout / v4l2 output | Planned |

---

## Credits

- [Butterchurn](https://github.com/jberg/butterchurn) — WebGL Milkdrop renderer by Jordan Berg
- [butterchurn-presets](https://github.com/jberg/butterchurn-presets) — bundled preset collection
- [SvelteKit](https://kit.svelte.dev) / [Svelte 5](https://svelte.dev)
- [Electron](https://www.electronjs.org)
- Preset authors — listed in each `.milk` file; used under their respective licenses

## License

MIT — see [LICENSE](LICENSE).

---

*Made with ❤️ by [kushiemoon-dev](https://github.com/kushiemoon-dev)*
