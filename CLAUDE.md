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

Filename convention: `ggml-{model}.bin`. The two models the app currently
ships with: `small` and `large-v3-turbo`. Default is `large-v3-turbo`.

**Models are no longer bundled with release artifacts.** Each app build
installs at ~50–80 MB; the models (~728 MB combined) are hosted on the
dedicated `models-v1` GitHub Release of this repo and **downloaded by the
app on first launch** (or whenever a model file is missing). The Welcome
window shows a progress bar during the download and gates its "Get
Started" button until everything is present.

- Stable model URLs:
  - `https://github.com/AccessDevops/S2Tui/releases/download/models-v1/ggml-small.bin`
  - `https://github.com/AccessDevops/S2Tui/releases/download/models-v1/ggml-large-v3-turbo.bin`
- Storage location at runtime (path derived from `bundle.identifier` in
  `tauri.conf.json` = `com.accessdevops.s2tui`):
  - macOS: `~/Library/Application Support/com.accessdevops.s2tui/models/`
  - Linux: `~/.local/share/com.accessdevops.s2tui/models/`
  - Windows: `%APPDATA%\com.accessdevops.s2tui\models\`
- The `models-v1` GitHub Release is **load-bearing** — do not delete it.
- Adding a new model = upload the file to that release, then add an entry
  to `MODEL_REGISTRY` in `src-tauri/src/commands.rs` (id, filename, URL,
  SHA-256, size) and the new id will appear automatically.

For local development, the **dev mode keeps reading from
`src-tauri/models/`** (unchanged from before) so a maintainer who
already has the binaries doesn't have to wait on the auto-download path.
If you don't have them locally, fetch them from the `models-v1` release
or from Hugging Face and rename:

```bash
# Example for small model
curl -L -o src-tauri/models/ggml-small.bin \
  https://github.com/AccessDevops/S2Tui/releases/download/models-v1/ggml-small.bin
# Or from upstream Hugging Face (same content):
curl -L -o src-tauri/models/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin
```

## Persisted state — backend is the single source of truth

Every value that survives a restart (`Settings` fields + `history`)
is owned by the **Rust backend**. The frontend Pinia stores are
caches that re-hydrate from backend on a single broadcast event.

### How to add a new persisted field

1. Add the field to `Settings` in `src-tauri/src/state.rs` with
   `#[serde(default)]` so existing settings.json files still load.
2. Add a `#[tauri::command] pub fn set_X(value, state, app: AppHandle)
   -> Result<(), String>` in `src-tauri/src/commands.rs`. End with
   `persist_and_broadcast(&state, &app)?;` — that one call writes
   `settings.json` AND emits `settings:changed`. Register the
   command in `src-tauri/src/lib.rs`.
3. Add the matching field on the TypeScript `Settings` interface in
   `src/stores/appStore.ts` and its mirror on `PersistedSettings`
   in `src/composables/useStore.ts`.
4. Wire the field into `useSettingsSync.refresh()` so it updates the
   Pinia cache on every `settings:changed` event.

That's the whole list. The cross-window sync handles itself — every
window listens to `settings:changed` once via `useSettingsSync` and
re-fetches the canonical Settings via `get_settings`.

### Rules to keep the discipline

- **Never** import from `@tauri-apps/plugin-store` in any `.ts`
  / `.vue` file. The JS plugin is gone from our app code; any
  reintroduction is a regression. `git grep "@tauri-apps/plugin-store"
  src/` should return zero hits.
- **Never** read or write settings.json directly from JS. All reads
  go through `invoke('get_settings')` (or the `loadSettings`
  helper that wraps it). All writes go through a Tauri setter
  command.
- **Never** add a per-slice listener for `settings:changed`. The
  event is global; one `useSettingsSync` per window is enough.
  Adding more listeners increases the chance of double-fetch
  thrashing without giving anything back.
- **Atomic mutators only**. A setter command must finish its
  in-memory mutation, persist to disk, and emit the broadcast all
  before returning. The pattern is encoded in
  `persist_and_broadcast` — use it, don't re-implement it.

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
