<div align="center">

# OpenDrop

### Multi-Deck Audio Visualizer

[![Pre-release](https://img.shields.io/badge/Status-Pre--release-orange?style=flat)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![Version](https://img.shields.io/badge/Version-0.1.0--alpha-blue?style=flat)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat&logo=linux&logoColor=black)](https://github.com/kushiemoon-dev/OpenDrop-VJ)
[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/kushiemoon-dev/OpenDrop-VJ)
[![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-24C8DB?style=flat&logo=tauri&logoColor=white)](https://tauri.app/)
[![ProjectM](https://img.shields.io/badge/ProjectM-4.1.6-purple)](https://github.com/projectM-visualizer/projectm)

**A NestDrop alternative for Linux and Windows**

Real-time audio visualization with MilkDrop presets, multi-deck mixing, and video output for OBS/VLC.

> **Note:** This is an alpha pre-release. Features may be incomplete or unstable.

</div>

---

## Features

- **Multi-Deck Visualization** — Up to 4 independent decks with crossfader mixing
- **MilkDrop Presets** — Full support for .milk and .prjm preset files via ProjectM 4.x
- **Audio Capture** — Native PipeWire/PulseAudio support (Linux), WASAPI (Windows)
- **Video Output** — v4l2loopback (Linux), Spout (Windows) for OBS/VLC integration
- **NDI Streaming** — Network video output (optional, requires NDI SDK)
- **MIDI Control** — Full MIDI mapping with learn mode and controller presets
- **Playlist Management** — Per-deck playlists with shuffle and auto-cycle
- **Compositor** — Blend modes (Normal, Add, Multiply, Screen, Overlay)
- **Multi-Monitor** — Fullscreen on any connected display
- **VU Meters** — Real-time audio level visualization

---

## Download

| Platform | Download |
|----------|----------|
| Linux (x64) | [opendrop-linux-x64.tar.gz](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases) |
| Windows (x64) | [opendrop-windows-x64.msi](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases) |

---

## Requirements

### Linux

```bash
# Arch Linux
sudo pacman -S projectm pipewire

# Ubuntu/Debian
sudo apt install libprojectm4 pipewire

# Video output (optional)
sudo pacman -S v4l2loopback-dkms   # Arch
sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="OpenDrop"
```

### Windows

- Windows 10/11 (x64)
- [Spout](https://spout.zeal.co/) for video output to OBS

---

## Build from Source

```bash
# Prerequisites
# - Rust 1.75+
# - Node.js 18+
# - pnpm
# - libprojectM-4 development files

# Clone
git clone https://github.com/kushiemoon-dev/OpenDrop-VJ.git
cd OpenDrop-VJ

# Install dependencies
pnpm install

# Development mode
pnpm tauri dev

# Release build
pnpm tauri build
```

---

## How It Works

```
┌─────────────────────────────────────────────────────────┐
│         Frontend (Svelte 5 / SvelteKit)                 │
│  AudioPanel, PresetBrowser, Playlist, Crossfader, VU   │
└────────────────┬────────────────────────────────────────┘
                 │ Tauri IPC Commands
                 ▼
┌─────────────────────────────────────────────────────────┐
│         Tauri Backend (Rust)                            │
│  AppState, AudioEngine, MIDI, Crossfader, Compositor   │
└──┬──────────────────────────────────────────────┬───────┘
   │ Spawn (JSON IPC)                             │ Audio
   ▼                                              ▼
┌──────────────────┐  ┌──────────────────┐   ┌──────────┐
│ Renderer Deck 0  │  │ Renderer Deck 1-3│   │ PipeWire │
│ ProjectM+OpenGL  │  │ ProjectM+OpenGL  │   │ Capture  │
│ + Video Output   │  │ + Video Output   │   └──────────┘
└────────┬─────────┘  └──────────────────┘
         │ glReadPixels
         ▼
┌──────────────────┐
│ /dev/video10     │ → OBS/VLC
│ v4l2loopback     │
└──────────────────┘
```

---

## Video Output Setup

### Linux (v4l2loopback)

```bash
# Load kernel module
sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="OpenDrop"

# Verify device
ls -la /dev/video10

# In OBS: Sources → Video Capture Device → OpenDrop
```

### Windows (Spout)

1. Download [SpoutLibrary.dll](https://github.com/leadedge/Spout2/releases)
2. Place in `C:\Program Files\OpenDrop\`
3. In OBS: Sources → Spout2 Capture → OpenDrop

---

## MIDI Controller Support

Built-in presets for popular controllers:
- **Generic DJ** — Universal 2-deck mapping
- **Akai APC Mini** — Grid-based control
- **Novation Launchpad** — Pad-based preset switching
- **Korg nanoKONTROL2** — Fader-based mixing

Custom mappings can be created via the MIDI Learn mode.

---

## Configuration

Settings are stored in:
- **Linux:** `~/.config/opendrop/`
- **Windows:** `%APPDATA%\opendrop\`

| Setting | Default | Description |
|---------|---------|-------------|
| `preset_path` | `/usr/share/projectM/presets` | Default preset directory |
| `audio_device` | Auto | Audio capture device |
| `video_output` | Disabled | v4l2/Spout output |

---

## Tech Stack

| Component | Technology |
|-----------|------------|
| Backend | Rust, Tauri 2.x |
| Frontend | Svelte 5, SvelteKit, TypeScript |
| Visualization | ProjectM 4.x, OpenGL 3.3 |
| Audio | PipeWire, CPAL, PulseAudio |
| MIDI | midir |
| Video Out | v4l2 (Linux), Spout (Windows), NDI (optional) |

---

## Project Structure

```
opendrop/
├── src/                    # Svelte frontend
│   ├── lib/components/     # UI components
│   └── routes/             # SvelteKit pages
├── src-tauri/              # Tauri backend
│   └── src/lib.rs          # Main Rust logic
├── crates/
│   ├── opendrop-core/      # Core library (audio, video, MIDI)
│   ├── opendrop-renderer/  # OpenGL renderer process
│   ├── projectm-rs/        # Safe ProjectM wrapper
│   └── projectm-sys/       # ProjectM FFI bindings
└── static/                 # Static assets
```

---

## Credits

- [ProjectM](https://github.com/projectM-visualizer/projectm) — MilkDrop visualization engine
- [Tauri](https://tauri.app/) — Desktop application framework
- [Svelte](https://svelte.dev/) — Frontend framework
- [midir](https://github.com/Boddlnagg/midir) — MIDI library for Rust

---

## Disclaimer

- This software is for **personal and educational use only**
- Respect the licenses of preset files you use
- Support the original preset creators
- No warranty is provided; use at your own risk

---

## License

MIT License - See [LICENSE](LICENSE) for details.

---

<div align="center">

Made with ❤️ by [kushiemoon-dev](https://github.com/kushiemoon-dev)

</div>
