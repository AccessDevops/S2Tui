# S2Tui

Free, Local and Open Source speech-to-text application powered by Whisper. Your voice stays on your machine. Compatible with Windows, Linux and MacOS

## Features

- **100% Free** - For ever
- **100% Local** - No internet required, no data leaves your computer
- **Floating Overlay** - Minimal, always-on-top microphone button
- **Auto-copy** - Text is inserted directly in your clipboard
- **Global Shortcut** - Cmd/Ctrl+Shift+Space from anywhere
- **Multi-language** - 12+ languages with auto-detection

## Installation

Please check the [Release](https://github.com/AccessDevops/S2Tui/releases)

## Requirement

|OS|Requirements|
|:---|:---|
|macOs|Must have [GPU Metal optimization](https://developer.apple.com/metal/) installed by default since 2012, fully compatible since 2021 with the M series|
|Windows|Must have [Vulkan](https://www.vulkan.org/tools#vulkan-gpu-resources) installed by default with NVidia, AMD and Intel driver since 2012|
|Linux debian and ubuntu |`sudo apt install mesa-vulkan-drivers nvidia-driver-550`|
|Linux fedora |`sudo dnf install mesa-vulkan-drivers akmod-nvidia`|

## Usage

1. Click the microphone button to start recording
2. Speak
3. Click again to stop and transcribe
4. Text is automatically inserted at your cursor

**Shortcut:** `Cmd+Shift+Space` (macOS) or `Ctrl+Shift+Space` (Windows/Linux)

## Contributing and Development 

```bash
# Install dependencies
npm install

# Download Whisper model (simplified naming without quantization suffix)
mkdir -p src-tauri/models
curl -L -o src-tauri/models/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin

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
