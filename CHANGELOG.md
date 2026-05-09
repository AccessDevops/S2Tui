# Changelog

All notable changes to S2Tui will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.8] - 2026-05-09

### Added
- **Custom Whisper model import.** Settings → Whisper Models now has
  an `Add` button that opens a native file picker. The selected
  `.bin` is validated against Whisper's architectural invariants
  (magic bytes, n_audio_ctx, n_text_ctx, n_mels, vocab range,
  quantisation type — all checked from the first 48 bytes of the
  file, < 1 ms wall time) and imported with a user-chosen display
  name. Built-in and user-imported models share the same row UI.
- **Per-model Disable / Delete actions** on every row. Disable
  removes the model from the cycle shortcut while keeping it
  manually selectable; Delete removes a custom-model entry from
  the imported list (the file on disk is preserved). Built-in
  models cannot be deleted but can be disabled.
- **Auto-detection of English-only model variants** (`.en` files,
  `n_vocab=51864`). Their per-model language whitelist is
  pre-populated with `["auto", "en"]` so the cycle shortcut and
  the chip picker treat non-English languages as not allowed for
  the model. Row carries an "EN only" pill.
- **Runtime broken-model detection.** A model that fails to load
  is marked broken in-session, surfaces a red pill and a Retry
  button on its row, and is skipped by the cycle shortcuts —
  instead of crashing the worker.
- **Memory pre-flight at import time.** The validator emits a
  soft `HighMemoryUse` warning when the candidate model file
  exceeds 60% of available RAM, so users on tight machines see
  the trade-off before they import.
- **Anti-hallucination knobs** — `set_suppress_nst(true)` drops
  bracketed/parenthesised non-speech tokens (`[Music]`,
  `[Applause]`, `(typing)`, sigh tokens) inherited from Whisper's
  subtitle/podcast training data. Per-segment
  `no_speech_probability > 0.6` post-decode filter catches the
  residual ghost text Whisper invents on silent audio tails.
- **AppImage bundle for the Linux release**
  (`S2Tui_<version>_amd64.AppImage`). Single file that runs on
  Arch, Fedora, Debian, Ubuntu and any distro with glibc ≥ 2.35
  — no manual dependency install required, just `chmod +x` and
  run.

### Changed
- **Upgrade `whisper-rs` 0.13.2 → 0.16.0** (`whisper-rs-sys` 0.11.1
  → 0.15.0). Pulls in upstream whisper.cpp anti-hallucination
  fix (PR #2629), faster Metal kernels, support for newer
  quantisation variants (q4_K, q5_K, q6_K, IQ family). Public API
  surface for our usage is unchanged; the segment-collection
  loop adapts to the renamed `state.get_segment(i)` accessor.
- **Backend = single source of truth for persisted state.**
  `settings.json` is now read and written exclusively from Rust
  (via `Settings::persist` + `tauri-plugin-store`'s `StoreExt`).
  Every Tauri setter command persists atomically and emits a
  `settings:changed` event; every window listens once via the
  new `useSettingsSync` composable and re-fetches the canonical
  state. Eliminates the recurring "I forgot to refresh slice X
  in window Y" desync bug class. Documented as a convention in
  `CLAUDE.md`.
- **Tauri 2.9 → 2.11** to align Rust crates with the
  `@tauri-apps/api` 2.11 JS package.
- **Whisper.cpp upgrade enables the v3 model invariant**
  (`n_mels=128` for large-v3 / large-v3-turbo / distil-large-v3).
  The compatibility validator accepts both 80 and 128.
- **Quantisation-version-encoded ftype decoded properly.**
  whisper.cpp encodes `raw_ftype = qntvr × 1000 + ftype` on
  disk; our validator decodes it instead of rejecting real
  q5_x q5_K models.

### Fixed
- **Cycle shortcut now sees newly-imported custom models** in the
  main window without requiring a Settings window close+reopen.
  Root cause: per-window Pinia stores stayed out of sync because
  the main window's `settings:updated` listener didn't refresh
  the `models` slice. Fixed by the Option B refactor (above).
- **Settings → Whisper Models tab is no longer empty on open.**
  Same Pinia-per-window root cause; fixed by the same refactor.
- **Custom-model load no longer fails on UUID-keyed ids.** The
  backend now resolves model paths via a shared helper
  (`resolve_model_path`) that consults built-in `MODEL_REGISTRY`
  first and `Settings.user_models` second, instead of blindly
  synthesising `<models_dir>/ggml-<id>.bin`.
- **Compatibility error JSON now exposes hparams in camelCase**
  to the frontend. `serde(rename_all = "camelCase")` on tagged
  enums only renames the variant tag, not the inner struct
  fields; we add `rename_all_fields = "camelCase"` so the
  details panel actually shows the values it claims to.
- **Release pipeline `release.yml` Windows job** uses the proper
  Qt-installer unattended flags (`--accept-licenses`,
  `--default-answer`, `--confirm-command install`) for the
  Vulkan SDK installer instead of the legacy NSIS `/S` flag.
  Adds a 25-minute step timeout so a future installer
  regression fails fast instead of burning the GitHub default
  6-hour job timeout.

## [0.1.7] - 2026-05-07

### Added
- Supported language registry expanded from 14 to 64. Settings now
  exposes a tier-aware (high / medium) chip cycle with a searchable
  picker for adding favourites; `auto` is pinned and cannot be
  removed.
- `language-first` cycle mode for the language toggle shortcut: when
  the next favourite isn't supported by the active model, the most
  capable compatible model is loaded automatically. The legacy
  `model-first` mode (cycle stays within the active model's
  whitelist) remains the default.
- Whisper models are no longer bundled in the installer. They are
  downloaded from the dedicated `models-v1` GitHub Release on first
  launch (~728 MB total, one-time). Each app build is now ~50–80 MB
  instead of ~800 MB.
- Welcome window shows a per-model progress bar while the first-launch
  download is running, and gates its "Get started" button until both
  models are present.
- Mic button surfaces download progress when the welcome window is
  closed: blue ring around the button reflecting cumulative percent,
  dimmed flag/icon, click intercept, and a permanent badge above the
  mic showing `Downloading… N%`.
- Settings → Whisper Models tab shows live per-row state during a
  download: `Downloading… X%` with inline progress bar and byte
  counter, `Pending` for queued entries, and a `Retry` action when a
  download fails (the only recovery surface once welcome has been
  closed).
- Cycle shortcuts (language and model) early-return with the same
  download badge text instead of attempting to load a `.partial`
  file.

### Fixed
- Flag SVGs now render in packaged builds. The default Vite asset
  inlining converted small SVGs to `data:` URLs, which CSP rejected
  on first paint; inlining is disabled and Tauri's CSP allows the
  asset protocol.
- Settings footer pulls the app version from Tauri at runtime
  (`getVersion()`), so the displayed version always matches the
  built binary instead of the hardcoded literal that was visibly
  stale after every release.

### Changed
- Welcome window: download progress card now sits above the GPU
  status section, matching what the user actually waits on at first
  launch.
- Release workflow no longer copies Whisper models into the Windows
  portable ZIP, the Linux portable tarball, or the RPM payload —
  those archives now ship without bundled models, the app downloads
  them on first launch like every other artifact.

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
