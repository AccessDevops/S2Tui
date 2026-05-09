import { onScopeDispose } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useAppStore, type ModelInfo, type Language, type ModelCapabilities } from "../stores/appStore";
import type { PersistedSettings } from "./useStore";

// One sync listener per window. Subscribes to `settings:changed`
// (emitted by every Rust setter via `persist_and_broadcast`) and
// re-fetches the canonical Settings via `get_settings`. Replaces the
// per-slice "did I plumb the listener?" wiring that historically led
// to silent desync between windows when a new persisted field was
// added without updating every listener.
//
// Usage (called once per window in onMounted/initListeners):
//
//     const sync = useSettingsSync();
//     await sync.attach();
//
// `attach()` performs an initial refresh + registers the listener +
// returns an unlisten callback. The composable also auto-cleans the
// listener via `onScopeDispose` so callers in component lifecycles
// don't have to track it manually.
//
// This is the second layer of the "backend = source of truth"
// architecture (cf. CLAUDE.md "Persisted state" section). The first
// layer is on the Rust side — every mutator call is atomic
// (`persist_and_broadcast`), so by the time `settings:changed`
// fires, the disk and AppState are already consistent.

interface ModelInfoResponse {
  id: string;
  displayName: string;
  kind: "builtin" | "custom";
  capabilities: ModelCapabilities;
  disabled: boolean;
  broken: boolean;
  path?: string;
  url?: string;
  filename?: string;
  present: boolean;
}

function formatBytes(n: number): string {
  if (n <= 0) return "0 MB";
  const mb = n / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}

function toModelInfo(r: ModelInfoResponse): ModelInfo {
  return {
    id: r.id,
    name: r.displayName,
    size: formatBytes(r.capabilities.fileSizeBytes),
    sizeBytes: r.capabilities.fileSizeBytes,
    downloaded: r.present,
    downloading: false,
    progress: r.present ? 100 : 0,
    bundled: false,
    kind: r.kind,
    disabled: r.disabled,
    broken: r.broken,
    path: r.path,
    capabilities: r.capabilities,
  };
}

export function useSettingsSync() {
  const store = useAppStore();
  const unlistens: UnlistenFn[] = [];

  /** Pull the full canonical state from the backend and update
   *  every Pinia slice that mirrors persisted data (`settings`,
   *  `models`, `history`, `vulkanWarningDismissed`,
   *  `welcomeDismissed`). Called on first attach + on every
   *  `settings:changed` event. */
  async function refresh(): Promise<void> {
    const persisted = await invoke<PersistedSettings>("get_settings");
    store.updateSettings({
      language: persisted.language,
      model: persisted.model,
      autoCopy: persisted.autoCopy,
      shortcut: persisted.shortcut,
      languageToggleShortcut: persisted.languageToggleShortcut ?? "",
      modelToggleShortcut: persisted.modelToggleShortcut ?? "",
      favoriteLanguages: persisted.favoriteLanguages ?? [],
      modelLanguages: (persisted.modelLanguages as Record<string, Language[]>) ?? {},
      languageCycleMode: persisted.languageCycleMode ?? "model-first",
      userModels: persisted.userModels ?? [],
      disabledModels: persisted.disabledModels ?? [],
    });
    store.setHistory(persisted.history ?? []);
    store.setVulkanWarningDismissed(persisted.vulkanWarningDismissed ?? false);
    store.setWelcomeDismissed(persisted.welcomeDismissed ?? false);

    // The merged model list lives behind a separate command
    // (`list_all_models`) because it includes runtime-only fields
    // (`present`, `broken`, `disabled`) that aren't part of the
    // persisted Settings. Refreshing it here keeps the rule "every
    // settings:changed event = full sync" intact without forcing
    // every caller to remember the second invoke.
    try {
      const list = await invoke<ModelInfoResponse[]>("list_all_models");
      store.setModels(list.map(toModelInfo));
    } catch (err) {
      console.error("Failed to refresh model list:", err);
    }
  }

  /** Initial fetch + listener registration. Returns an unlisten
   *  callback for callers that prefer to manage their own lifecycle;
   *  otherwise `onScopeDispose` will clean up at component unmount. */
  async function attach(): Promise<UnlistenFn> {
    await refresh();
    const unlisten = await listen("settings:changed", () => {
      refresh().catch((e) => console.error("settings sync refresh failed:", e));
    });
    unlistens.push(unlisten);
    return unlisten;
  }

  onScopeDispose(() => {
    for (const u of unlistens) {
      try {
        u();
      } catch {
        // Listener already gone — nothing to clean up.
      }
    }
  });

  return { attach, refresh };
}
