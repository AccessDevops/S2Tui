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

## [0.1.6] - 2026-05-06

### Added
- Optional global shortcut to cycle between favorite languages without
  opening Settings.
- Optional global shortcut to cycle between downloaded models. When the
  next model doesn't support the active language, the language auto-bumps
  to the first favorite that model accepts.
- Per-model language whitelist in Settings. The language cycle stays within
  the languages enabled on the currently selected model — the model is
  sticky and never switches via the language shortcut.
- Visual language indicator on the mic button: round country flag for the
  active language, replaced by a thick coloured ring to convey
  listening/processing/error state. Mic icon colour adapts to flag
  brightness.

### Fixed
- Cross-window settings sync. Toggle-shortcut mutations from the main
  window now reflect live in the open Settings window (and vice versa)
  via a `settings:updated` Tauri event. The two windows used to keep
  separate Pinia stores so the shortcut listener could read stale
  favourites and cycle through 14 languages instead of the 2 selected.
- Race in `setLanguage`/`setModel` that broadcast the cross-window event
  before persisting to disk, causing the language toggle to look like it
  needed two presses.
- Toast notifications above the mic button now wrap onto multiple lines
  (max-width 84 px, font 10 px, leading-tight) so messages fit inside the
  90×100 px overlay window without truncation.

### Changed
- Release workflow rewritten to be tag-driven, idempotent and to produce
  a single GitHub Release per tag with the full artifact set
  (macOS arm64 dmg, macOS x64 dmg, Windows NSIS exe, MSI, portable zip,
  Linux deb, RPM, portable tar.gz). Previous flow could produce two
  releases when a cancelled run left a draft behind.
- `src-tauri/.cargo/config.toml` documents that `[target.cfg.env]` is
  silently ignored by cargo. `CMAKE_GENERATOR=Ninja` for Windows is now
  set in the CI step instead of the cargo config.

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
