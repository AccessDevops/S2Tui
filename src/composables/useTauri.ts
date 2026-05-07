import { invoke } from "@tauri-apps/api/core";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  useAppStore,
  type ModelId,
  type Language,
  type SystemHealth,
  type GpuStatus,
  LANGUAGE_DISPLAY_NAMES,
} from "../stores/appStore";
import { loadSettings, saveSettings, addHistoryEntry, loadHistory } from "./useStore";

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

  // Each Tauri window owns its own Pinia store, so a setter that mutates
  // settings in one window does not reach the other. Broadcast a Tauri event
  // and have every window reload from persistence — same pattern we already
  // use for `history:updated`.
  async function broadcastSettingsUpdate(): Promise<void> {
    try {
      await emit("settings:updated", null);
    } catch (err) {
      console.error("Failed to broadcast settings update:", err);
    }
  }

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
  // Order matters: persist to disk BEFORE broadcasting `settings:updated`.
  // The cross-window listener reads `loadSettings()` from disk, so if we
  // broadcast first the listener would see the previous on-disk value and
  // overwrite the in-memory store with stale state. This race manifested
  // as the language toggle "needing two presses" because every other press
  // was instantly reverted by our own broadcast handler.
  async function setModel(name: ModelId) {
    try {
      await invoke("set_model", { name });
      store.updateSettings({ model: name });
      await saveSettings({ model: name });
      await broadcastSettingsUpdate();
    } catch (error) {
      console.error("Failed to set model:", error);
    }
  }

  async function setLanguage(lang: Language) {
    try {
      await invoke("set_language", { lang });
      store.updateSettings({ language: lang });
      await saveSettings({ language: lang });
      await broadcastSettingsUpdate();
    } catch (error) {
      console.error("Failed to set language:", error);
    }
  }

  async function setShortcut(shortcut: string): Promise<void> {
    try {
      await invoke("set_shortcut", { shortcut });
      store.updateSettings({ shortcut });
      await saveSettings({ shortcut });
      await broadcastSettingsUpdate();
    } catch (error) {
      console.error("Failed to set shortcut:", error);
      throw error;
    }
  }

  async function setLanguageToggleShortcut(shortcut: string): Promise<void> {
    try {
      await invoke("set_language_toggle_shortcut", { shortcut });
      store.updateSettings({ languageToggleShortcut: shortcut });
      await saveSettings({ languageToggleShortcut: shortcut });
      await broadcastSettingsUpdate();
    } catch (error) {
      console.error("Failed to set language toggle shortcut:", error);
      throw error;
    }
  }

  async function setModelToggleShortcut(shortcut: string): Promise<void> {
    try {
      await invoke("set_model_toggle_shortcut", { shortcut });
      store.updateSettings({ modelToggleShortcut: shortcut });
      await saveSettings({ modelToggleShortcut: shortcut });
      await broadcastSettingsUpdate();
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
      await saveSettings({ favoriteLanguages: next });
      await broadcastSettingsUpdate();
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
      await saveSettings({ modelLanguages: next });
      await broadcastSettingsUpdate();
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
      await saveSettings({ languageCycleMode: mode });
      await broadcastSettingsUpdate();
    } catch (error) {
      console.error("Failed to set language cycle mode:", error);
      throw error;
    }
  }

  // Commands - Model Management
  async function loadWhisperModel(model: ModelId) {
    try {
      await invoke("load_whisper_model", { model });
      store.updateSettings({ model });
      await saveSettings({ model });
      await broadcastSettingsUpdate();
    } catch (error) {
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

      // Update settings
      store.updateSettings({ model });
      await saveSettings({ model });
      await broadcastSettingsUpdate();

      return result;
    } catch (error) {
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

      // Detect actually available models on disk (not just persisted state)
      let availableModels = await getAvailableModels();

      // Reset all models to not downloaded first
      for (const model of store.models) {
        store.setModelDownloaded(model.id, false);
      }

      // Mark only actually available models as downloaded
      for (const modelId of availableModels) {
        store.setModelDownloaded(modelId as ModelId, true);
      }

      // Load history
      const history = await loadHistory();
      store.setHistory(history);

      // Ask the backend which models the app needs and which are missing.
      // Models are no longer bundled with the app — they get downloaded on
      // first launch from the `models-v1` GitHub Release.
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

      // Seed the modelDownload Pinia slice so the mic ring + Settings rows
      // can render progress *before* the first byte arrives. Items start as
      // `pending`; the Tauri event listeners (registered in `initListeners`)
      // promote each to `downloading` / `done` / `error` as the backend
      // emits events.
      if (missingModels.length > 0) {
        for (const m of missingModels) {
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
        // Nothing to download → make sure stale state from a previous
        // session (HMR / re-init) doesn't keep the UI in download mode.
        store.clearModelDownload();
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

    // Model-download lifecycle. The WelcomePage owns its own listeners for
    // its in-window progress bars; these copies live on the main window so
    // the mic ring and the Settings → Models tab can react even when the
    // welcome window is closed (the trigger that motivated this whole UX).
    unlistenFns.push(await listen<{
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
    }));
    unlistenFns.push(await listen<{ model: string }>(
      "model:download:complete",
      (e) => {
        store.upsertModelDownloadItem(e.payload.model, {
          status: "done",
          percent: 100,
          errorMessage: undefined,
        });
        // Mark the model as available so the rest of the UI (model picker,
        // cycle shortcut) treats it as ready immediately. The `bytesReceived`
        // bump keeps the cumulative percent honest.
        store.setModelDownloaded(e.payload.model as ModelId, true);
      },
    ));
    unlistenFns.push(await listen<{ model: string; message: string }>(
      "model:download:error",
      (e) => {
        store.upsertModelDownloadItem(e.payload.model, {
          status: "error",
          errorMessage: e.payload.message,
        });
      },
    ));

    // Vulkan warning dismissed event (from vulkan warning window)
    unlistenFns.push(await listen<{ permanent: boolean }>("vulkan-warning:dismissed", async (event) => {
      if (event.payload.permanent) {
        store.setVulkanWarningDismissed(true);
        await saveSettings({ vulkanWarningDismissed: true });
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

    // Welcome dismissed event (from welcome window)
    unlistenFns.push(await listen<{ permanent: boolean }>("welcome:dismissed", async (event) => {
      if (event.payload.permanent) {
        store.setWelcomeDismissed(true);
        await saveSettings({ welcomeDismissed: true });
      }
    }));

    // Open settings event (from tray menu)
    unlistenFns.push(await listen("open:settings", () => {
      openSettings();
    }));

    // Cross-window settings sync. Whichever window mutates settings (Settings
    // window editing favorites, main window cycling via toggle shortcut, …)
    // emits `settings:updated`; every window reloads from persistence so its
    // local Pinia store no longer drifts. Without this, the toggle listeners
    // below would read stale `favoriteLanguages` and cycle through 14 entries
    // instead of the 2 the user picked in Settings.
    unlistenFns.push(await listen("settings:updated", async () => {
      try {
        const persisted = await loadSettings();
        store.updateSettings({
          language: persisted.language,
          model: persisted.model,
          autoCopy: persisted.autoCopy,
          shortcut: persisted.shortcut,
          languageToggleShortcut: persisted.languageToggleShortcut ?? "",
          modelToggleShortcut: persisted.modelToggleShortcut ?? "",
          favoriteLanguages: persisted.favoriteLanguages ?? store.settings.favoriteLanguages,
          modelLanguages: persisted.modelLanguages ?? {},
          languageCycleMode: persisted.languageCycleMode ?? "model-first",
        });
      } catch (err) {
        console.error("Failed to reload settings on settings:updated event:", err);
      }
    }));

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
        const downloaded = store.models.filter((m) => m.downloaded);
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

      const downloaded = store.models.filter((m) => m.downloaded);
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
