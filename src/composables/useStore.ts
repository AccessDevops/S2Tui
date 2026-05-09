import { invoke } from "@tauri-apps/api/core";
import type { Settings, HistoryEntry, ModelId } from "../stores/appStore";

/// All persistence is now backend-driven (cf. CLAUDE.md "Persisted
/// state"). This module is a thin wrapper around the Tauri commands
/// `get_settings`, `add_history_entry`, `clear_history` for callers
/// that prefer the historical helper names. New code should call
/// `invoke` directly or, better, use `useSettingsSync` so both the
/// fetch and the cross-window cache invalidation are covered in one
/// call.
///
/// The previous `tauri-plugin-store` JS pathway is gone: the Rust
/// backend reads/writes `settings.json` itself, atomically inside
/// every setter command, and emits `settings:changed` so all
/// windows refresh their Pinia cache. This eliminates the per-slice
/// "did I plumb the listener?" bug class.

export interface PersistedSettings extends Settings {
  history: HistoryEntry[];
  vulkanWarningDismissed: boolean;
  welcomeDismissed: boolean;
}

/** Fetch the full Settings struct from the Rust backend. Single
 *  source of truth — no JSON-on-disk parsing in the frontend. */
export async function loadSettings(): Promise<PersistedSettings> {
  return await invoke<PersistedSettings>("get_settings");
}

/** Read just the history slice. Implemented as a projection of
 *  `loadSettings()` to avoid a duplicate command. The full Settings
 *  payload is small (< 5 KB even with 20 history entries) so the
 *  extra fields are inexpensive. */
export async function loadHistory(): Promise<HistoryEntry[]> {
  const settings = await loadSettings();
  return settings.history ?? [];
}

/** Persist a freshly transcribed clip. Returns the entry the
 *  backend created (with its assigned id + timestamp) so the
 *  caller can wire it into the UI without a second round-trip. */
export async function addHistoryEntry(
  text: string,
  modelId?: string,
  durationMs?: number,
): Promise<HistoryEntry> {
  return await invoke<HistoryEntry>("add_history_entry", {
    entry: {
      text,
      modelId: modelId as ModelId | undefined,
      durationMs,
    },
  });
}

/** Wipe the history list. */
export async function clearHistory(): Promise<void> {
  await invoke("clear_history");
}
