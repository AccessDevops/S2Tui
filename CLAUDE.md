# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

S2Tui is a local Speech-to-Text desktop application for Windows, macOS, and Linux. It provides a floating overlay with a microphone button that captures audio, transcribes it using Whisper.cpp locally with GPU acceleration, and can auto-paste the text into any application.

## Tech Stack

- **Frontend**: Vue 3 + TypeScript + Tailwind CSS + Vite + Pinia
- **Backend**: Tauri 2 + Rust
- **Speech Recognition**: whisper-rs (Whisper.cpp Rust bindings)
- **GPU Acceleration**: Vulkan (Windows/Linux), Metal (macOS)
- **Audio Capture**: cpal
- **macOS Integration**: objc2, core-foundation (for accessibility API, window management)

## Development Commands

```bash
# Start development mode - use the appropriate command for your OS:
npm run tauri:dev:windows  # Windows (with Vulkan GPU support)
npm run tauri:dev:macos    # macOS (with Metal GPU support)

# Build for production
npm run tauri build

# Type-check Rust code only
cd src-tauri && cargo check

# Type-check frontend only
vue-tsc --noEmit
```

## Architecture

### Two Windows
- **Main window** (`index.html`): Transparent overlay with microphone button, always-on-top
- **Settings window** (`settings.html`): Configuration panel opened from overlay

### Frontend Structure
- `src/stores/appStore.ts` - Central Pinia store for app state (status, transcripts, settings, history)
- `src/composables/useTauri.ts` - Tauri command invocations and event listeners
- `src/composables/useStore.ts` - Settings persistence via tauri-plugin-store
- `src/components/Overlay.vue` - Main overlay UI
- `src/pages/SettingsPage.vue` - Settings panel

### Backend Structure (src-tauri/src/)
- `lib.rs` - App setup, plugin registration, global shortcut setup
- `commands.rs` - All Tauri commands (start_listen, stop_listen, load_whisper_model, etc.)
- `state.rs` - AppState struct with Whisper engine and audio capture
- `audio/` - Audio capture (cpal) and VAD
- `whisper/` - Whisper.cpp integration via whisper-rs
- `insertion/` - Text insertion via macOS Accessibility API

### Event Flow
1. User clicks mic button or triggers global shortcut
2. Frontend calls `start_listen` command
3. Backend starts audio capture, emits `vad:level` events
4. When user stops, audio is sent to Whisper
5. Backend emits `transcript:partial` then `transcript:final`
6. Frontend can auto-copy or auto-paste based on settings

## Key Patterns

### Tauri 2 API
Use `@tauri-apps/api/webviewWindow` for window operations:
```typescript
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
const window = getCurrentWebviewWindow();
```

### Plugin Imports
Use dynamic imports for Tauri plugins to avoid blocking module load:
```typescript
const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
```

### Settings Persistence
Settings are stored at:
- macOS: `~/Library/Application Support/com.s2tui.desktop/settings.json`

## Whisper Models

Models are stored in `src-tauri/models/` with naming convention `ggml-{model}.bin`:
- Default model: `large-v3-turbo`
- Available: small, large-v3-turbo

Models are downloaded from Hugging Face during CI/CD and bundled with the application. For local development, download the quantized versions and rename them:
```bash
# Example for small model
curl -L -o src-tauri/models/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin
```

## Platform Requirements

### Windows
- **GPU drivers with Vulkan support**: NVIDIA, AMD, or Intel drivers that support Vulkan
- **Ninja build tool**: Required for CMake (auto-configured in `.cargo/config.toml`)

### macOS
- **Microphone permission**: Required for audio capture
- **Accessibility permission**: Required for text insertion via AXUIElement

### Linux
- **Vulkan drivers**: Install via package manager (mesa-vulkan-drivers, nvidia-vulkan-icd, etc.)
- **Microphone access**: May require PulseAudio/PipeWire configuration

## Build Prerequisites

- Rust toolchain (via rustup)
- Node.js 18+
- Ninja (Windows - for CMake builds)
- Xcode Command Line Tools (macOS)
