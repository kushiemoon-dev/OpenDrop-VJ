# Changelog

All notable changes to OpenDrop will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
