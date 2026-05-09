import { invoke } from "@tauri-apps/api/core";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  useAppStore,
  type ModelId,
  type ModelInfo,
  type Language,
  type SystemHealth,
  type GpuStatus,
  type ModelCapabilities,
  LANGUAGE_DISPLAY_NAMES,
} from "../stores/appStore";
import { loadSettings, addHistoryEntry, loadHistory } from "./useStore";
import { useModelDownloadTracker } from "./useModelDownloadTracker";
import { useSettingsSync } from "./useSettingsSync";

// Platform-aware settings window opener
export async function openSettings() {
  // Check if settings window already exists
  const existingWindow = await WebviewWindow.getByLabel("settings");
  if (existingWindow) {
    await existingWindow.setFocus();
    return;
  }

  // Detect platform for platform-specific options
  let platform = "unknown";
  try {
    const os = await import("@tauri-apps/plugin-os");
    platform = os.platform();
  } catch {
    // Platform detection failed, use defaults
  }

  // Build window options based on platform
  const isMacOS = platform === "macos";

  const settingsWindow = new WebviewWindow("settings", {
    url: "settings.html",
    title: "Settings - S2Tui",
    width: 700,
    height: 550,
    minWidth: 600,
    minHeight: 450,
    resizable: true,
    center: true,
    transparent: false,
    shadow: true,
    // No native decorations - we use custom title bar with close button
    decorations: false,
    ...(isMacOS ? { titleBarStyle: "overlay" as const } : {}),
  });

  settingsWindow.once("tauri://error", (e) => {
    console.error("Failed to create settings window:", e);
  });
}

type ListenMode = "toggle" | "push-to-talk" | "voice-activated";

// Module-level flags to prevent duplicate initialization and actions
let listenersInitialized = false;
let isActionInProgress = false;
const unlistenFns: UnlistenFn[] = [];

interface VadLevelPayload {
  rms: number;
}

interface TranscriptPayload {
  text: string;
  model?: string;
  transcribeDurationMs?: number;
}

export function useTauri() {
  const store = useAppStore();

  // Commands - Audio
  async function startListen(mode: ListenMode = "toggle") {
    try {
      await invoke("start_listen", { mode });
    } catch (error) {
      console.error("Failed to start listening:", error);
      store.setStatus("error");
    }
  }

  async function stopListen() {
    try {
      // Set processing status immediately - this is controlled by frontend
      // to ensure it stays blue for the entire transcription duration
      store.setStatus("processing");

      // Wait for transcription to complete (this blocks until Whisper is done)
      await invoke("stop_listen");

      // Transcription complete - status will be set to idle by transcript:final handler
    } catch (error) {
      console.error("Failed to stop listening:", error);
      store.setStatus("idle");
    }
  }

  // Commands - Settings.
  //
  // Each wrapper now does the bare minimum: invoke the backend command,
  // optimistically update the local Pinia cache for snappiness, return.
  //
  // The Rust setter is responsible for persisting `settings.json`
  // atomically (via `Settings::persist`) and emitting `settings:changed`
  // so every window's `useSettingsSync` re-fetches the canonical state.
  // The previous JS-side `saveSettings()` + `broadcastSettingsUpdate()`
  // dance is gone — backend is the single source of truth.
  async function setModel(name: ModelId) {
    try {
      await invoke("set_model", { name });
      store.updateSettings({ model: name });
    } catch (error) {
      console.error("Failed to set model:", error);
    }
  }

  async function setLanguage(lang: Language) {
    try {
      await invoke("set_language", { lang });
      store.updateSettings({ language: lang });
    } catch (error) {
      console.error("Failed to set language:", error);
    }
  }

  async function setShortcut(shortcut: string): Promise<void> {
    try {
      await invoke("set_shortcut", { shortcut });
      store.updateSettings({ shortcut });
    } catch (error) {
      console.error("Failed to set shortcut:", error);
      throw error;
    }
  }

  async function setLanguageToggleShortcut(shortcut: string): Promise<void> {
    try {
      await invoke("set_language_toggle_shortcut", { shortcut });
      store.updateSettings({ languageToggleShortcut: shortcut });
    } catch (error) {
      console.error("Failed to set language toggle shortcut:", error);
      throw error;
    }
  }

  async function setModelToggleShortcut(shortcut: string): Promise<void> {
    try {
      await invoke("set_model_toggle_shortcut", { shortcut });
      store.updateSettings({ modelToggleShortcut: shortcut });
    } catch (error) {
      console.error("Failed to set model toggle shortcut:", error);
      throw error;
    }
  }

  async function setFavoriteLanguages(languages: Language[]): Promise<void> {
    // `auto` is the auto-detect sentinel and must always be available in
    // the cycle list. Re-injecting it here is the safety net for any
    // caller that forgot (and self-heals settings.json files where the
    // user removed it via the picker before we made auto unremovable).
    const next = languages.includes("auto") ? languages : ["auto", ...languages];
    try {
      await invoke("set_favorite_languages", { languages: next });
      store.updateSettings({ favoriteLanguages: next });
    } catch (error) {
      console.error("Failed to set favorite languages:", error);
      throw error;
    }
  }

  async function setModelLanguages(model: string, languages: Language[]): Promise<void> {
    try {
      await invoke("set_model_languages", { model, languages });
      const next = { ...store.settings.modelLanguages, [model]: languages };
      store.updateSettings({ modelLanguages: next });
    } catch (error) {
      console.error("Failed to set model languages:", error);
      throw error;
    }
  }

  async function setLanguageCycleMode(
    mode: "model-first" | "language-first",
  ): Promise<void> {
    try {
      await invoke("set_language_cycle_mode", { mode });
      store.updateSettings({ languageCycleMode: mode });
    } catch (error) {
      console.error("Failed to set language cycle mode:", error);
      throw error;
    }
  }

  // Commands - Model Management
  async function loadWhisperModel(model: ModelId) {
    try {
      await invoke("load_whisper_model", { model });
      // Successful load — clear any prior broken flag for this id.
      // Backend persists + emits settings:changed, frontend cache
      // catches up via the unified resync.
      store.clearModelBroken(model);
      store.updateSettings({ model });
    } catch (error) {
      // Runtime safety net: mark the model "broken" locally so cycle
      // shortcuts skip it (Step 7 filter) and the row UI surfaces a
      // Retry button (Step 6). The reason text comes straight from
      // whisper-rs / our backend wrapper. Cleared on next successful
      // load or by app restart (broken state isn't persisted).
      const reason = error instanceof Error ? error.message : String(error);
      store.markModelBroken(model, reason);
      console.error("Failed to load whisper model:", error);
      throw error;
    }
  }

  async function isModelLoaded(): Promise<boolean> {
    try {
      return await invoke<boolean>("is_model_loaded");
    } catch (error) {
      console.error("Failed to check if model is loaded:", error);
      return false;
    }
  }

  // Commands - Permissions
  async function checkPermissions() {
    try {
      const perms = await invoke<{ microphone: boolean }>("check_permissions");
      store.setPermissions(perms);
      return perms;
    } catch (error) {
      console.error("Failed to check permissions:", error);
      return { microphone: false };
    }
  }

  async function requestMicrophonePermission(): Promise<boolean> {
    try {
      const granted = await invoke<boolean>("request_microphone_permission");
      if (granted) {
        store.setPermissions({ microphone: true });
      }
      return granted;
    } catch (error) {
      console.error("Failed to request microphone permission:", error);
      return false;
    }
  }

  // Commands - Model detection
  async function getAvailableModels(): Promise<string[]> {
    try {
      return await invoke<string[]>("get_available_models");
    } catch (error) {
      console.error("Failed to get available models:", error);
      return [];
    }
  }

  // ---- Custom model registry --------------------------------------------
  // Mirrors the Rust `ModelInfoResponse` returned by `list_all_models`.
  // Field names match the Rust serde `rename_all = "camelCase"` output.
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

  /** Format a byte count as a human-friendly MB/GB string. Mirrors the
   *  rendering used in Settings → Models rows so the seed matches the
   *  hand-written entries it replaced. */
  function formatBytes(n: number): string {
    if (n <= 0) return "0 MB";
    const mb = n / (1024 * 1024);
    if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
    return `${Math.round(mb)} MB`;
  }

  /** Translate a backend `ModelInfoResponse` into the frontend
   *  `ModelInfo` shape used by the store. Pure mapping, no side
   *  effects. */
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

  /** Fetch the merged built-in + user-imported model list from the
   *  backend and seed the Pinia `models` slice. Called once on app
   *  boot, and again whenever `settings:updated` fires (so the
   *  Settings window reflects an import done in the main window). */
  async function refreshModelList(): Promise<void> {
    try {
      const list = await invoke<ModelInfoResponse[]>("list_all_models");
      store.setModels(list.map(toModelInfo));
    } catch (error) {
      console.error("Failed to list models:", error);
    }
  }

  /** Run the file-format validator on a candidate path. Resolves
   *  with the validation result, rejects with a structured
   *  ModelCompatError-shaped object the dialog can switch on by
   *  `kind`. */
  async function validateCustomModel(path: string): Promise<unknown> {
    return invoke("validate_custom_model", { path });
  }

  /** Persist a freshly-validated model. Returns the backend's
   *  `UserModel` payload (with the assigned UUID + capabilities)
   *  so the caller can append it to the local persisted list and
   *  trigger the cross-window broadcast. */
  interface BackendUserModel {
    id: string;
    displayName: string;
    path: string;
    addedAt: number;
    capabilities: ModelCapabilities;
  }
  async function addCustomModel(name: string, path: string): Promise<BackendUserModel> {
    const created = await invoke<BackendUserModel>("add_custom_model", { name, path });
    // Backend already persisted + emitted settings:changed. We just
    // re-seed the local model list so the new row is visible
    // immediately without waiting for the event round-trip.
    await refreshModelList();
    return created;
  }

  /** Remove a user-imported model. Backend handles the auto-switch
   *  to first-available if the deleted model was active. Returns
   *  the new active model id (or null if no switch was needed). */
  async function removeCustomModel(id: string): Promise<string | null> {
    const newActive = await invoke<string | null>("remove_custom_model", { id });
    // Same as addCustomModel: backend persisted + emitted; we just
    // refresh the local list and update the active-model pointer
    // optimistically.
    await refreshModelList();
    if (newActive) {
      store.updateSettings({ model: newActive as ModelId });
    }
    return newActive;
  }

  /** Toggle the disabled flag on a model id (built-in or custom). */
  async function setModelDisabled(id: string, disabled: boolean): Promise<void> {
    await invoke("set_model_disabled", { id, disabled });
    store.setModelDisabledLocal(id, disabled);
  }

  // Commands - System Health
  async function checkSystemHealth(): Promise<SystemHealth> {
    try {
      const health = await invoke<SystemHealth>("check_system_health");
      store.setSystemHealth(health);
      return health;
    } catch (error) {
      console.error("Failed to check system health:", error);
      throw error;
    }
  }

  async function getGpuStatus(): Promise<GpuStatus> {
    try {
      const status = await invoke<GpuStatus>("get_gpu_status");
      store.setGpuStatus(status);
      return status;
    } catch (error) {
      console.error("Failed to get GPU status:", error);
      throw error;
    }
  }

  interface ModelLoadResult {
    success: boolean;
    usingGpu: boolean;
    backend: string;
    fallbackUsed: boolean;
  }

  async function loadWhisperModelWithOptions(model: ModelId, forceCpu: boolean): Promise<ModelLoadResult> {
    try {
      const result = await invoke<ModelLoadResult>("load_whisper_model_with_options", {
        model,
        forceCpu,
      });

      // Update GPU status in store
      store.setGpuStatus({
        usingGpu: result.usingGpu,
        backend: result.backend,
        fallbackUsed: result.fallbackUsed,
      });

      // Successful load — clear any broken flag. Backend persists
      // settings.model + emits settings:changed; we just keep the
      // local cache optimistically up-to-date.
      store.clearModelBroken(model);
      store.updateSettings({ model });

      return result;
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      store.markModelBroken(model, reason);
      console.error("Failed to load whisper model with options:", error);
      throw error;
    }
  }

  // Load persisted settings and initialize app
  async function initApp() {
    try {
      // Load persisted settings
      const persisted = await loadSettings();

      // Update store with persisted settings
      store.updateSettings({
        language: persisted.language,
        model: persisted.model,
        autoCopy: persisted.autoCopy,
        shortcut: persisted.shortcut,
        languageToggleShortcut: persisted.languageToggleShortcut ?? "",
        modelToggleShortcut: persisted.modelToggleShortcut ?? "",
        favoriteLanguages: persisted.favoriteLanguages ?? store.settings.favoriteLanguages,
        modelLanguages: persisted.modelLanguages ?? {},
      });

      // Push the persisted language to the backend. Without this, Whisper stays
      // on its default (auto-detect) until the user re-selects the language.
      try {
        await setLanguage(persisted.language);
      } catch (err) {
        console.error("Failed to sync persisted language to backend:", err);
      }

      // Sync persisted toggle config to backend so shortcuts are registered
      // and per-model filters are honored without the user re-saving anything.
      try {
        await invoke("set_favorite_languages", { languages: store.settings.favoriteLanguages });
        for (const [model, langs] of Object.entries(store.settings.modelLanguages)) {
          await invoke("set_model_languages", { model, languages: langs });
        }
        if (persisted.languageToggleShortcut) {
          await invoke("set_language_toggle_shortcut", { shortcut: persisted.languageToggleShortcut });
        }
        if (persisted.modelToggleShortcut) {
          await invoke("set_model_toggle_shortcut", { shortcut: persisted.modelToggleShortcut });
        }
        if (persisted.languageCycleMode) {
          await invoke("set_language_cycle_mode", { mode: persisted.languageCycleMode });
        }
      } catch (err) {
        console.error("Failed to sync toggle config to backend:", err);
      }

      // Load persisted system health settings
      store.setVulkanWarningDismissed(persisted.vulkanWarningDismissed ?? false);
      store.setWelcomeDismissed(persisted.welcomeDismissed ?? false);

      // Check system health (GPU/Vulkan availability)
      let systemHealth: SystemHealth | null = null;
      try {
        systemHealth = await checkSystemHealth();
      } catch {
        // System health check failed, continue without GPU info
      }

      // Seed the models slice from the backend's merged built-in +
      // user-imported list. This replaces the v0.1.7 hardcoded init
      // (two ModelInfo entries hardcoded in the store) — the source
      // of truth now lives in Rust's `MODEL_REGISTRY` + persisted
      // `userModels`. The `present` flag returned by the backend
      // doubles as our `downloaded` boolean.
      await refreshModelList();

      // For now we still call `getAvailableModels` because downstream
      // welcome/init logic uses it to decide whether the welcome
      // dialog should run. Cheap, no harm.
      let availableModels = await getAvailableModels();

      // Load history
      const history = await loadHistory();
      store.setHistory(history);

      // Ask the backend which models the app needs and which are missing.
      // Models are no longer bundled with the app — they get downloaded on
      // first launch from the `models-v1` GitHub Release. The seed +
      // listener wiring lives in `useModelDownloadTracker` (called from
      // `initListeners` so listeners attach exactly once); we just need
      // the missing-list here to drive the welcome modal + auto-load.
      interface RequiredModelInfo {
        id: string;
        displayName: string;
        filename: string;
        sizeBytes: number;
        url: string;
        present: boolean;
      }
      let missingModels: RequiredModelInfo[] = [];
      try {
        const required = await invoke<RequiredModelInfo[]>("list_required_models");
        missingModels = required.filter((m) => !m.present);
      } catch (err) {
        console.error("Failed to query required models:", err);
      }

      // Show Vulkan warning if:
      // - We checked system health
      // - Vulkan is not available
      // - User hasn't dismissed the warning
      // - We're on Windows or Linux (not macOS which uses Metal)
      const DEBUG_FORCE_VULKAN_WARNING = false;
      const shouldShowVulkanWarning =
        DEBUG_FORCE_VULKAN_WARNING ||
        (systemHealth &&
        !systemHealth.vulkanAvailable &&
        !persisted.vulkanWarningDismissed &&
        systemHealth.osInfo.platform !== "macos");

      // Welcome window also doubles as the model-download UI on first launch.
      // Force-open it whenever a model is missing, regardless of whether the
      // user previously dismissed it — they need the progress feedback.
      if (shouldShowVulkanWarning) {
        store.openVulkanWarningModal();
      } else if (!persisted.welcomeDismissed || missingModels.length > 0) {
        store.openWelcomeModal();
      }

      // Sequentially download every missing model in the background. The
      // welcome window subscribes to `model:download:*` events to render
      // the progress bars and gates its "Get started" button on completion.
      // We deliberately don't await this — the user shouldn't be staring at
      // a frozen screen, and the loadWhisperModel below will be retried
      // after the download succeeds.
      const startDownloads = (async () => {
        for (const m of missingModels) {
          try {
            await invoke("download_model", { model: m.id });
          } catch (err) {
            console.error(`Model ${m.id} download failed:`, err);
            return; // stop the chain, leave the rest pending
          }
        }
      })();

      // Decide whether the model can be loaded right now.
      const tryLoadCurrentModel = async () => {
        // Refresh the available list — a download may have just landed.
        availableModels = await getAvailableModels();
        for (const id of availableModels) {
          store.setModelDownloaded(id as ModelId, true);
        }
        if (!availableModels.includes(persisted.model)) return;
        try {
          const forceCpu =
            systemHealth &&
            !systemHealth.vulkanAvailable &&
            systemHealth.osInfo.platform !== "macos";
          await loadWhisperModelWithOptions(persisted.model, forceCpu ?? false);
        } catch {
          // Model loading failed, user can select model in settings
        }
      };

      if (missingModels.length === 0) {
        await tryLoadCurrentModel();
      } else {
        // Wait for the download chain to finish (or fail) before loading.
        // Don't block initApp itself — kick a follow-up task instead.
        startDownloads.finally(tryLoadCurrentModel);
      }
    } catch (error) {
      console.error("Failed to initialize app:", error);
    }
  }

  // Initialize event listeners
  async function initListeners() {
    // Prevent duplicate initialization
    if (listenersInitialized) {
      return;
    }
    listenersInitialized = true;

    // Audio events
    unlistenFns.push(await listen<VadLevelPayload>("vad:level", (event) => {
      store.setVuLevel(event.payload.rms);
    }));

    // State changes from backend - only handle "listening" state here
    // "processing" is set by stopListen(), "idle" is set by transcript:final handler
    unlistenFns.push(await listen<string>("state:change", (event) => {
      const newStatus = event.payload as any;
      // Only update status for listening state - processing/idle are handled elsewhere
      if (newStatus === "listening") {
        store.setStatus(newStatus);
      }
    }));

    unlistenFns.push(await listen<TranscriptPayload>("transcript:partial", (event) => {
      store.setPartialTranscript(event.payload.text);
    }));

    unlistenFns.push(await listen<TranscriptPayload>("transcript:final", async (event) => {
      const { text, model, transcribeDurationMs } = event.payload;
      store.setLastTranscript(text);

      // Transcription complete - set status to idle
      store.setStatus("idle");

      // Add to history (in-memory and persisted)
      if (text.trim()) {
        store.addToHistory(text, model as any, transcribeDurationMs);

        // Persist history first, then emit event (so Settings can read the updated file)
        addHistoryEntry(text, model, transcribeDurationMs)
          .then(() => {
            // Emit event only after persistence is complete
            emit("history:updated").catch((error) => {
              console.error("Failed to emit history:updated event:", error);
            });
          })
          .catch((error) => {
            console.error("Failed to persist history:", error);
          });

        // Copy to clipboard if autoCopy is enabled
        if (store.settings.autoCopy) {
          try {
            const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
            await writeText(text);
            store.triggerCopyNotification();
          } catch (error) {
            console.error("Failed to copy to clipboard:", error);
          }
        }
      }
    }));

    // Permission events
    unlistenFns.push(await listen<string>("permission:required", (event) => {
      if (event.payload === "microphone") {
        store.openPermissionGuide("microphone");
      }
    }));

    // Permission granted event (from permission window)
    unlistenFns.push(await listen<{ type: string }>("permission:granted", (event) => {
      if (event.payload.type === "microphone") {
        store.setPermissions({ microphone: true });
        // Reset status to idle so user can try again
        store.setStatus("idle");
      }
    }));

    // Model loaded event
    unlistenFns.push(await listen<string>("model:loaded", (event) => {
      store.updateSettings({ model: event.payload as ModelId });
    }));

    // Model-download lifecycle. Both the seed (initial `list_required_models`
    // call) and the 3 listeners are encapsulated in `useModelDownloadTracker`,
    // which the Settings window also calls — see SettingsPage.vue.
    await useModelDownloadTracker().attach();

    // Vulkan warning dismissed event (from vulkan warning window).
    // The backend `set_vulkan_warning_dismissed` command handles
    // both persistence and the cross-window broadcast atomically.
    unlistenFns.push(await listen<{ permanent: boolean }>("vulkan-warning:dismissed", async (event) => {
      if (event.payload.permanent) {
        store.setVulkanWarningDismissed(true);
        await invoke("set_vulkan_warning_dismissed", { dismissed: true });
      }
    }));

    // Vulkan warning closed event - open welcome modal after vulkan warning closes
    unlistenFns.push(await listen<{ showWelcome: boolean }>("vulkan-warning:closed", async (event) => {
      if (event.payload.showWelcome && !store.welcomeDismissed) {
        // Small delay for smooth transition
        setTimeout(() => {
          store.openWelcomeModal();
        }, 300);
      }
    }));

    // Welcome dismissed event (from welcome window). Same atomic
    // backend pattern as the vulkan-warning case above.
    unlistenFns.push(await listen<{ permanent: boolean }>("welcome:dismissed", async (event) => {
      if (event.payload.permanent) {
        store.setWelcomeDismissed(true);
        await invoke("set_welcome_dismissed", { dismissed: true });
      }
    }));

    // Open settings event (from tray menu)
    unlistenFns.push(await listen("open:settings", () => {
      openSettings();
    }));

    // Cross-window settings sync. Backend = single source of truth:
    // every Rust setter command persists settings.json + emits
    // `settings:changed`. The `useSettingsSync` composable listens
    // for that event once, re-fetches the canonical Settings via
    // `get_settings`, and updates every Pinia slice that mirrors
    // persisted state. New persisted fields require zero changes
    // here — they're picked up by `get_settings` automatically.
    unlistenFns.push(await useSettingsSync().attach());

    // Drop the parenthetical descriptor from a model display name (e.g.
    // "Large V3 Turbo (Best)" -> "Large V3 Turbo") so the toast inside the
    // 90×100 px overlay window stays readable on one or two lines.
    const shortModelName = (name: string): string => name.replace(/\s*\([^)]*\)\s*$/, "");

    // Language cycle shortcut. Two behaviours selectable from Settings via
    // `languageCycleMode`:
    //   - "model-first" (v0.1.6 default): cycle is restricted to favourites
    //     the active model supports. Model is sticky — use the model shortcut
    //     to change it.
    //   - "language-first" (rusak47's keyboard-layout request): cycle walks
    //     through every favourite; if the current model doesn't support the
    //     next language, auto-switch to the most-capable compatible model
    //     (largest sizeBytes) before changing the language.
    unlistenFns.push(await listen("shortcut:toggle-language", async () => {
      // While any required model is still downloading we'd hit a `.partial`
      // file or a model that isn't loaded yet — surface the same toast the
      // mic-button click does, instead of silently failing.
      if (store.modelDownload.active) {
        store.showToggleNotification(
          `Downloading models — ${store.modelDownloadCumulativePercent}%`,
        );
        return;
      }

      const favorites = store.settings.favoriteLanguages;
      if (favorites.length < 2) {
        store.showToggleNotification("Add 2+ favorite languages");
        return;
      }

      const modelLangs = store.settings.modelLanguages;
      const mode = store.settings.languageCycleMode;

      if (mode === "language-first") {
        // ============ MODE: LANGUAGE-FIRST ============================
        // Filter out disabled (user-excluded from cycle) and broken
        // (failed to load this session) models. Built-ins and customs
        // get the same treatment per the locked-in UX answer.
        const downloaded = store.models.filter(
          (m) => m.downloaded && !m.disabled && !m.broken,
        );
        if (downloaded.length === 0) {
          store.showToggleNotification("No model");
          return;
        }

        // Pick the most-capable compatible model for a given language. Tie
        // is broken by sizeBytes desc — proxy of "more capable". Returns
        // null if no downloaded model accepts the language.
        const bestModelFor = (lang: Language) => {
          const compatible = downloaded.filter((m) => {
            const list = modelLangs[m.id];
            return list === undefined || list.includes(lang);
          });
          if (compatible.length === 0) return null;
          return [...compatible].sort((a, b) => b.sizeBytes - a.sizeBytes)[0];
        };

        const startIndex = Math.max(0, favorites.indexOf(store.settings.language));
        for (let step = 1; step <= favorites.length; step++) {
          const candidate = favorites[(startIndex + step) % favorites.length];
          const target = bestModelFor(candidate);
          if (!target) continue; // no model supports this language → skip

          const needsModelSwitch = target.id !== store.settings.model;
          // Mid-recording, switching model would kill the in-flight
          // transcription. Pure language change is safe (whisper-rs reads
          // params at the start of each `state.full()`).
          if (needsModelSwitch && store.status !== "idle") {
            store.showToggleNotification("Stop recording before switching model");
            return;
          }

          try {
            const label = shortModelName(target.name);
            if (needsModelSwitch) {
              store.showToggleNotification(`Loading ${label}…`);
              await loadWhisperModel(target.id);
            }
            await setLanguage(candidate);
            const langDisplay = LANGUAGE_DISPLAY_NAMES[candidate] || candidate;
            store.showToggleNotification(
              needsModelSwitch ? `${label} · ${langDisplay}` : langDisplay,
            );
          } catch (error) {
            console.error("Failed to toggle language:", error);
            store.showToggleNotification("Toggle failed");
          }
          return;
        }

        store.showToggleNotification("No model fits favorites");
        return;
      }

      // ============ MODE: MODEL-FIRST (default, v0.1.6 behaviour) =====
      const currentModel = store.settings.model;
      const allowedForModel = (lang: Language): boolean => {
        const list = modelLangs[currentModel];
        if (list === undefined) return true;
        return list.includes(lang);
      };

      const eligible = favorites.filter(allowedForModel);
      if (eligible.length === 0) {
        store.showToggleNotification("No favorite for this model");
        return;
      }
      if (eligible.length === 1) {
        const only = eligible[0];
        if (store.settings.language === only) {
          store.showToggleNotification("Only 1 favorite for this model");
          return;
        }
        try {
          await setLanguage(only);
          store.showToggleNotification(LANGUAGE_DISPLAY_NAMES[only] || only);
        } catch (error) {
          console.error("Failed to toggle language:", error);
          store.showToggleNotification("Toggle failed");
        }
        return;
      }

      const currentIndex = eligible.indexOf(store.settings.language);
      const nextIndex = currentIndex >= 0 ? (currentIndex + 1) % eligible.length : 0;
      const nextLang = eligible[nextIndex];

      try {
        await setLanguage(nextLang);
        store.showToggleNotification(LANGUAGE_DISPLAY_NAMES[nextLang] || nextLang);
      } catch (error) {
        console.error("Failed to toggle language:", error);
        store.showToggleNotification("Toggle failed");
      }
    }));

    // Model cycle shortcut: cycles through ALL downloaded models. If the next
    // model doesn't support the current language, it auto-bumps the language
    // to the first favorite that model accepts (so model switching never gets
    // blocked just because the active language isn't whitelisted there).
    // Skips models whose whitelist excludes every favorite — they would
    // otherwise leave the user with no usable language at all.
    unlistenFns.push(await listen("shortcut:toggle-model", async () => {
      if (store.modelDownload.active) {
        store.showToggleNotification(
          `Downloading models — ${store.modelDownloadCumulativePercent}%`,
        );
        return;
      }

      if (store.status !== "idle") return;

      // Same filter chain as the language-first branch above. Disabled
      // and broken models stay out of the cycle uniformly.
      const downloaded = store.models.filter(
        (m) => m.downloaded && !m.disabled && !m.broken,
      );
      if (downloaded.length < 2) {
        store.showToggleNotification(
          downloaded.length === 0 ? "No model" : "Only 1 model",
        );
        return;
      }

      const modelLangs = store.settings.modelLanguages;
      const favorites = store.settings.favoriteLanguages;
      const currentLang = store.settings.language;
      const currentIdx = downloaded.findIndex((m) => m.id === store.settings.model);
      const startFrom = Math.max(0, currentIdx);

      for (let step = 1; step <= downloaded.length; step++) {
        const candidate = downloaded[(startFrom + step) % downloaded.length];
        const list = modelLangs[candidate.id];
        const supportsCurrent = list === undefined || list.includes(currentLang);
        const fallbackLang = list === undefined ? null : favorites.find((l) => list.includes(l));

        if (!supportsCurrent && !fallbackLang) {
          // Whitelist exists and excludes every favorite → unusable model.
          continue;
        }

        try {
          const nextLang = supportsCurrent ? null : (fallbackLang as Language);
          const label = shortModelName(candidate.name);
          store.showToggleNotification(`Loading ${label}…`);
          await loadWhisperModel(candidate.id);
          if (nextLang !== null) {
            await setLanguage(nextLang);
            store.showToggleNotification(`${label} · ${LANGUAGE_DISPLAY_NAMES[nextLang]}`);
          } else {
            store.showToggleNotification(label);
          }
        } catch (error) {
          console.error("Failed to toggle model:", error);
          store.showToggleNotification("Model load failed");
        }
        return;
      }

      store.showToggleNotification("No model fits favorites");
    }));

    // Global shortcut listener - with guard to prevent duplicate actions
    unlistenFns.push(await listen("shortcut:triggered", async () => {
      // Prevent duplicate actions if already processing
      if (isActionInProgress) {
        return;
      }

      if (store.status === "listening") {
        isActionInProgress = true;
        try {
          await stopListen();
        } finally {
          isActionInProgress = false;
        }
      } else if (store.status === "idle") {
        isActionInProgress = true;
        try {
          await startListen("toggle");
        } finally {
          isActionInProgress = false;
        }
      }
    }));

    // Check permissions on init
    checkPermissions();

    // Initialize app (load persisted settings and model)
    initApp();
  }

  return {
    // Audio
    startListen,
    stopListen,
    // Settings
    setModel,
    setLanguage,
    setShortcut,
    setLanguageToggleShortcut,
    setModelToggleShortcut,
    setFavoriteLanguages,
    setModelLanguages,
    setLanguageCycleMode,
    // Models
    loadWhisperModel,
    loadWhisperModelWithOptions,
    isModelLoaded,
    getAvailableModels,
    refreshModelList,
    validateCustomModel,
    addCustomModel,
    removeCustomModel,
    setModelDisabled,
    // System Health
    checkSystemHealth,
    getGpuStatus,
    // Permissions
    checkPermissions,
    requestMicrophonePermission,
    // Init
    initListeners,
    initApp,
  };
}
