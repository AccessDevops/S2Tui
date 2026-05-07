import { onScopeDispose } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useAppStore, type ModelId } from "../stores/appStore";

interface RequiredModelInfo {
  id: string;
  displayName: string;
  filename: string;
  sizeBytes: number;
  url: string;
  present: boolean;
}

// Wires a window's Pinia `modelDownload` slice to the backend's
// `model:download:*` lifecycle events. Each Tauri webview owns its own
// JS context (and therefore its own Pinia store), so every window that
// needs to react to downloads must call this once on mount.
//
// The composable handles three things in one shot:
//   1. Seeds the slice with `pending` items derived from
//      `list_required_models` so UI surfaces (mic ring, Settings rows)
//      can render even *before* the first byte arrives.
//   2. Syncs the `models` array's `downloaded` flag to disk truth —
//      relevant for windows like Settings whose store init defaults
//      `downloaded: true` and would otherwise show stale `Use` buttons.
//   3. Subscribes to `model:download:progress|complete|error` and
//      promotes items through the slice's state machine.
//
// Listeners auto-clean via `onScopeDispose`, so the caller doesn't
// have to manage them.
export function useModelDownloadTracker() {
  const store = useAppStore();
  const unlistens: UnlistenFn[] = [];

  async function attach() {
    try {
      const required = await invoke<RequiredModelInfo[]>("list_required_models");
      const presentIds = new Set(
        required.filter((m) => m.present).map((m) => m.id),
      );
      for (const m of store.models) {
        store.setModelDownloaded(m.id, presentIds.has(m.id));
      }
      const missing = required.filter((m) => !m.present);
      if (missing.length > 0) {
        for (const m of missing) {
          store.upsertModelDownloadItem(m.id, {
            displayName: m.displayName,
            sizeBytes: m.sizeBytes,
            status: "pending",
            bytesReceived: 0,
            percent: 0,
            errorMessage: undefined,
          });
        }
      } else {
        store.clearModelDownload();
      }
    } catch (err) {
      console.error("Failed to seed model download state:", err);
    }

    unlistens.push(
      await listen<{
        model: string;
        bytesReceived: number;
        totalBytes: number;
        percent: number;
      }>("model:download:progress", (e) => {
        store.upsertModelDownloadItem(e.payload.model, {
          status: "downloading",
          bytesReceived: e.payload.bytesReceived,
          sizeBytes: e.payload.totalBytes,
          percent: e.payload.percent,
          errorMessage: undefined,
        });
      }),
    );
    unlistens.push(
      await listen<{ model: string }>("model:download:complete", (e) => {
        store.upsertModelDownloadItem(e.payload.model, {
          status: "done",
          percent: 100,
          errorMessage: undefined,
        });
        store.setModelDownloaded(e.payload.model as ModelId, true);
      }),
    );
    unlistens.push(
      await listen<{ model: string; message: string }>(
        "model:download:error",
        (e) => {
          store.upsertModelDownloadItem(e.payload.model, {
            status: "error",
            errorMessage: e.payload.message,
          });
        },
      ),
    );
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

  return { attach };
}
