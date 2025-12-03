# S2Tui

Local, Free and Open Source speech-to-text application powered by Whisper. Your voice stays on your machine.

## Features

- **100% Local** - No internet required, no data leaves your computer
- **100% Free** - For ever
- **Floating Overlay** - Minimal, always-on-top microphone button
- **Auto-paste** - Text is inserted directly where your cursor is
- **Global Shortcut** - Cmd/Ctrl+Shift+Space from anywhere
- **Multi-language** - 12+ languages with auto-detection

## Installation

### macOS

1. Download `.dmg` from [Releases](../../releases)
2. Drag S2Tui to Applications
3. Right-click → Open on first launch
4. Grant Microphone and Accessibility permissions

### Windows

Download `.exe` installer or portable `.zip` from [Releases](../../releases)

### Linux

```bash
# AppImage
chmod +x S2Tui_*.AppImage && ./S2Tui_*.AppImage

# Debian/Ubuntu
sudo dpkg -i S2Tui_*.deb
```

## Usage

1. Click the microphone button to start recording
2. Speak
3. Click again to stop and transcribe
4. Text is automatically inserted at your cursor

**Shortcut:** `Cmd+Shift+Space` (macOS) or `Ctrl+Shift+Space` (Windows/Linux)

## Development

```bash
# Install dependencies
npm install

# Download Whisper model
mkdir -p src-tauri/models
curl -L -o src-tauri/models/ggml-small-q5_0.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_0.bin

# Run
npm run tauri dev

# Build
npm run tauri build
```

## Tech Stack

- **Frontend:** Vue 3, TypeScript, Tailwind CSS
- **Backend:** Tauri 2, Rust
- **Speech:** whisper-rs (Whisper.cpp)

## License

MIT
