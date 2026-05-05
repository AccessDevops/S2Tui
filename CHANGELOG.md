# Changelog

All notable changes to S2Tui will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Multi-platform support (macOS, Windows, Linux)
- GitHub Actions CI/CD pipeline
- Windows installer (NSIS) and portable ZIP
- Linux AppImage and .deb packages

### Changed
- Text insertion now works on Windows and Linux via clipboard + Ctrl+V simulation

## [0.1.5] - 2026-05-05

### Fixed
- Selected language is now actually honored by the Whisper engine
  (previously the UI value was saved but never propagated to the
  transcription engine, which always auto-detected). Fixes #1.
- Persisted language is re-synced to the backend on every app start.
- `cargo test` no longer fails because of the obsolete `GpuBackend::Cuda`
  assertion left over from the CUDA removal.

### Changed
- Whisper transcription tuned to reduce hallucinations on silence and
  low-energy audio: deterministic decoding (`temperature = 0`,
  `temperature_inc = 0`), `no_speech_thold = 0.6`, `suppress_blank = true`.
- `transcribe()` now logs the active language for diagnostics.

## [0.1.0] - 2025-01-XX

### Added
- Initial release
- Local speech-to-text using Whisper.cpp
- Floating overlay with microphone button
- Auto-paste transcribed text
- Auto-copy to clipboard
- Global keyboard shortcut (Cmd/Ctrl+Shift+Space)
- Settings panel with:
  - Language selection (auto-detect + 12 languages)
  - Model management (download, select, delete)
  - Transcription history
  - Permissions status
- VU meter visualization
- Draggable window
- Bundled Whisper models (small, large-v3-turbo)
