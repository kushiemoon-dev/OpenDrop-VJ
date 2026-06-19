# OpenDrop VJ

### Real-time 2-deck Milkdrop visualizer — web-first, Electron-ready

[![Live Demo](https://img.shields.io/badge/demo-opendrop.kushie.dev-ff2d78?style=flat-square)](https://opendrop.kushie.dev)
[![Release](https://img.shields.io/github/v/release/kushiemoon-dev/OpenDrop-VJ?style=flat-square)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![License](https://img.shields.io/github/license/kushiemoon-dev/OpenDrop-VJ?style=flat-square)](LICENSE)

Open-source VJ tool powered by [Butterchurn](https://github.com/jberg/butterchurn) (WebGL Milkdrop).
Two independent decks, live crossfader, MIDI control, video loops, and a dedicated output window for
second monitors or OBS — no C++ or native toolchain required.

---

## Features

- **2 decks A/B** — independent Butterchurn (Milkdrop WebGL) instances
- **Live crossfader** — real-time opacity blend A↔B; output window follows the fader continuously
- **Playlists** — per-deck auto-cycle (sequential / shuffle), 2–120 s interval, prev / next
- **Beat-sync / BPM** — bass-energy beat detector drives preset transitions on the beat
- **MIDI mapping** — CC/note → crossfader, playlist controls *(Chromium / Electron only)*
- **Preset browser** — search, favorites ★, 1 754 built-in presets (lazy-loaded)
- **Video loops** — overlay MP4/WebM clips with opacity, beat-reactive cut/flash/hue/warp
- **Overlays** — composited SVG/text layers synced to the output
- **Output window** — detached fullscreen canvas for a second monitor or OBS Browser Source;
  crossfader, presets, overlays, and video loops sync live from the main window
- **Mixer layout** — grid preset browser alongside the decks for fast live switching
- **Audio sources** — mic, device picker, audio file, system audio (all platforms)
- **NDI output** *(Electron, optional)* — requires NDI SDK + grandiose native module
- **Export / import** playlists as JSON
- **Keyboard shortcuts** — ← → nudge crossfader, Tab switches active deck

---

## Stack

| Layer | Tech |
|-------|------|
| UI | SvelteKit 2 + Svelte 5 runes, TypeScript |
| Visualizer | [Butterchurn](https://github.com/jberg/butterchurn) (Milkdrop WebGL) |
| Audio | Web Audio API — `AudioContext`, `AnalyserNode`, AudioWorklets |
| MIDI | Web MIDI API |
| Build | Vite + `@sveltejs/adapter-static` (SPA, `ssr: false`) |
| Desktop | Electron 42 (optional — PCM audio streaming, native window, NDI) |

---

## Try it

**→ [opendrop.kushie.dev](https://opendrop.kushie.dev)** — no install required (Chrome / Edge recommended).

---

## Quick Start (self-hosted)

```bash
pnpm install
pnpm dev          # → http://localhost:1420
```

Click **▶ Start** to initialize the audio context, then pick a preset in the browser panel.

---

## Desktop App (Electron)

Download the latest AppImage (Linux) from the [Releases](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases) page,
or build from source:

```bash
pnpm electron:dev     # Vite dev + Electron in parallel
pnpm electron:build   # Build SPA → package with electron-builder
```

**Linux / Wayland (Hyprland etc.)** — pass flags when launching the packaged app:

```bash
./OpenDrop-VJ.AppImage --ozone-platform=wayland --no-sandbox
```

---

## System Audio Capture

Click the audio source button in the app — behaviour adapts per platform:

| Platform | Web | Electron |
|----------|-----|----------|
| **Windows** | Screen picker → "Share system audio" (Chrome) | Native loopback — no picker |
| **Linux** | Device picker → `.monitor` source (PipeWire / PulseAudio) | Same |
| **macOS** | Tab audio only | Install [BlackHole](https://github.com/ExistentialAudio/BlackHole) → device picker |

**Linux tip:** `bash scripts/setup-audio.sh` creates a named PipeWire virtual source ("OpenDrop - Son du PC")
instead of using a raw `.monitor` device.

---

## Output Window & OBS

### Second monitor (Electron)
Click **Open Output** in the app. The fullscreen canvas follows the crossfader, presets, overlays,
and video loops in real time.

> **Known limitation:** on Linux (Electron), audio reactivity in the output window may require
> re-selecting the audio device once after the first open. This is a known issue — re-pick the device
> once and it stays reactive for the session.

### OBS Browser Source (web)
1. Add a **Browser Source** in OBS
2. URL: `http://localhost:1420/output` (dev) or the packaged app URL
3. Width / Height: match your stream resolution

---

## Development

```bash
pnpm check          # svelte-check (TypeScript + Svelte)
pnpm test           # Vitest unit tests
pnpm test:coverage  # Coverage report
pnpm test:e2e       # Playwright E2E (requires pnpm dev running)
pnpm build          # Production SPA → build/
```

---

## Project Structure

```
src/
├── lib/
│   ├── engine/
│   │   ├── audio.ts        AudioEngine — AudioContext, sources, AudioWorklets, PCM streaming
│   │   ├── bpm.ts          BeatDetector — bass-energy beat detection
│   │   ├── deck.ts         Deck — Butterchurn instance wrapper
│   │   ├── midi.ts         MidiEngine — Web MIDI, CC/note mapping
│   │   ├── overlay.ts      Overlay types
│   │   ├── playlist.ts     PlaylistEngine — auto-cycle, shuffle, prev/next
│   │   ├── quality.ts      Quality tiers (low / medium / high)
│   │   ├── sync.ts         Cross-window state sync (BroadcastChannel / Electron IPC)
│   │   └── video-store.ts  Video loop clip registry
│   ├── components/         Svelte UI components
│   ├── presets/            Preset registry (butterchurn-presets, search, lazy-load)
│   └── video-loops/        Built-in video loop manifests + loader
└── routes/
    ├── +page.svelte        Main VJ controller (Stage / Mixer layouts)
    └── output/
        └── +page.svelte    Fullscreen output canvas (OBS / second monitor)

electron/
├── main.cjs                Main process (IPC relay, PCM audio bridge, app:// protocol, NDI)
└── preload.cjs             contextBridge → window.electronAPI

static/
├── capture-worklet.js      AudioWorklet — taps gainNode, posts Int16 PCM chunks to main
└── loopback-worklet.js     AudioWorklet — ring-buffer PCM injection for output window

e2e/
└── app.spec.ts             Playwright E2E tests
```

---

## Known Limitations

| Item | Status |
|------|--------|
| Audio reactivity in output (Linux) | Requires re-picking the device once per session — known issue |
| Web MIDI | Chromium / Electron only (not Firefox / Safari) |
| System audio on macOS (browser) | Tab audio only — install BlackHole for full capture |
| Windows / macOS builds | Not yet distributed — Linux AppImage only for now |
| Signed installers | Planned |
| 4-deck compositor | Planned |
| Spout / v4l2 output | Planned |

---

## Credits

- [Butterchurn](https://github.com/jberg/butterchurn) — WebGL Milkdrop renderer by Jordan Berg
- [butterchurn-presets](https://github.com/jberg/butterchurn-presets) — bundled preset collection
- [SvelteKit](https://kit.svelte.dev) / [Svelte 5](https://svelte.dev)
- [Electron](https://www.electronjs.org)
- Preset authors — listed in each `.milk` file; used under their respective licenses

---

## License

MIT — see [LICENSE](LICENSE).

---

*Made with ❤️ by [kushiemoon-dev](https://github.com/kushiemoon-dev)*
