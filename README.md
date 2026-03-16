<div align="center">

# OpenDrop VJ

### Multi-Deck Audio Visualizer

[![Release](https://img.shields.io/github/v/release/kushiemoon-dev/OpenDrop-VJ?style=flat&color=00f0ff)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![Downloads](https://img.shields.io/github/downloads/kushiemoon-dev/OpenDrop-VJ/total?style=flat&color=8b5cf6)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat&logo=linux&logoColor=black)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases)
[![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri_2-24C8DB?style=flat&logo=tauri&logoColor=white)](https://tauri.app/)
[![ProjectM](https://img.shields.io/badge/ProjectM-4.x-purple)](https://github.com/projectM-visualizer/projectm)
[![License](https://img.shields.io/github/license/kushiemoon-dev/OpenDrop-VJ?style=flat)](LICENSE)

**Open-source NestDrop alternative for Linux and Windows**

Real-time audio visualization with MilkDrop presets, multi-deck mixing, MIDI control, and video output for OBS/VLC.

[Download Latest Release](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/latest) · [Report Bug](https://github.com/kushiemoon-dev/OpenDrop-VJ/issues) · [Request Feature](https://github.com/kushiemoon-dev/OpenDrop-VJ/issues)

</div>

---

## Features

- **Multi-Deck Visualization** — Up to 4 independent decks with crossfader mixing
- **MilkDrop Presets** — Full support for .milk and .prjm preset files via ProjectM 4.x
- **Audio Capture** — Native PipeWire/PulseAudio (Linux), WASAPI loopback (Windows)
- **Video Output** — v4l2loopback (Linux), Spout (Windows) for OBS/VLC integration
- **NDI Streaming** — Network video output (optional, requires NDI SDK)
- **MIDI Control** — Full MIDI mapping with learn mode and controller presets
- **Playlist Management** — Per-deck playlists with shuffle and auto-cycle
- **Compositor** — Blend modes (Normal, Add, Multiply, Screen, Overlay)
- **Multi-Monitor** — Fullscreen on any connected display
- **Preset Browser** — Favorites, categories, tags, progressive loading for large collections
- **Theme Support** — Dark/Light mode with 6 customizable accent colors
- **VU Meters** — Real-time audio level visualization

---

## Download

| Platform | Format | Download |
|----------|--------|----------|
| Linux (x64) | AppImage | [OpenDrop_0.3.5_amd64.AppImage](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/latest) |
| Linux (x64) | Debian | [OpenDrop_0.3.5_amd64.deb](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/latest) |
| Windows (x64) | Installer | [OpenDrop_0.3.5_x64-setup.exe](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/latest) |
| Windows (x64) | MSI | [OpenDrop_0.3.5_x64_en-US.msi](https://github.com/kushiemoon-dev/OpenDrop-VJ/releases/latest) |

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
- GPU with OpenGL 3.3+ support
- [Spout](https://spout.zeal.co/) for video output to OBS (SpoutLibrary.dll bundled in installer)

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

# Build renderer sidecar
cargo build --release -p opendrop-renderer
mkdir -p src-tauri/binaries
cp target/release/opendrop-renderer src-tauri/binaries/opendrop-renderer-$(rustc -vV | grep host | cut -d' ' -f2)

# Development mode
pnpm tauri dev

# Release build
pnpm tauri build
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│         Frontend (Svelte 5 / SvelteKit)                 │
│  PresetBrowser, Playlist, Crossfader, MIDI, VU Meters  │
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
│ ProjectM+OpenGL  │  │ ProjectM+OpenGL  │   │ /WASAPI  │
│ + Video Output   │  │ + Video Output   │   └──────────┘
└────────┬─────────┘  └──────────────────┘
         │ glReadPixels
         ▼
┌──────────────────┐
│ v4l2 / Spout     │ → OBS / VLC
│ / NDI            │
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

SpoutLibrary.dll is bundled with the installer. In OBS:
1. Install the [Spout2 plugin for OBS](https://github.com/Off-World-Live/obs-spout2-plugin)
2. Sources → Spout2 Capture → Select "OpenDrop Deck 1"

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
- **Windows:** `%APPDATA%\OpenDrop\`

Preset and texture directories are configurable in Settings (gear icon).

---

## Tech Stack

| Component | Technology |
|-----------|------------|
| Backend | Rust, Tauri 2.x |
| Frontend | Svelte 5, SvelteKit, TypeScript |
| Visualization | ProjectM 4.x, OpenGL 3.3 |
| Audio | PipeWire/PulseAudio (Linux), WASAPI (Windows) |
| MIDI | midir |
| Video Out | v4l2 (Linux), Spout (Windows), NDI (cross-platform) |
| Icons | Lucide |

---

## Project Structure

```
OpenDrop-VJ/
├── src/                    # Svelte frontend
│   ├── lib/components/     # UI components (12)
│   ├── lib/stores/         # State management (Svelte 5 runes)
│   └── routes/             # SvelteKit pages
├── src-tauri/              # Tauri backend
│   └── src/lib.rs          # Main Rust logic
├── crates/
│   ├── opendrop-core/      # Core library (audio, video, MIDI)
│   ├── opendrop-renderer/  # OpenGL renderer process (sidecar)
│   ├── projectm-rs/        # Safe ProjectM wrapper
│   └── projectm-sys/       # ProjectM FFI bindings
└── assets/                 # Bundled presets & textures
```

---

## Credits

- [ProjectM](https://github.com/projectM-visualizer/projectm) — MilkDrop visualization engine
- [Tauri](https://tauri.app/) — Desktop application framework
- [Svelte](https://svelte.dev/) — Frontend framework
- [Spout2](https://github.com/leadedge/Spout2) — Video frame sharing (Windows)
- [midir](https://github.com/Boddlnagg/midir) — MIDI library for Rust
- [Lucide](https://lucide.dev/) — Icon library

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
