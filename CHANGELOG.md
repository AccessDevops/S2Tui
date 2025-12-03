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
