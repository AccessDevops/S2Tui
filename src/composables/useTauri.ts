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
    try {
      await invoke("set_favorite_languages", { languages });
      store.updateSettings({ favoriteLanguages: languages });
      await saveSettings({ favoriteLanguages: languages });
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
      const availableModels = await getAvailableModels();

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

      if (shouldShowVulkanWarning) {
        // Show Vulkan warning first - Welcome modal will be shown when it closes
        store.openVulkanWarningModal();
      } else if (!persisted.welcomeDismissed) {
        // No Vulkan warning needed - show welcome directly
        store.openWelcomeModal();
      }

      // Load the whisper model if available
      if (availableModels.includes(persisted.model)) {
        try {
          // On Windows/Linux without Vulkan, force CPU mode
          // On macOS, Metal is always available so we never force CPU
          const forceCpu = systemHealth && !systemHealth.vulkanAvailable && systemHealth.osInfo.platform !== "macos";

          // Use the load function with GPU/CPU control
          await loadWhisperModelWithOptions(persisted.model, forceCpu ?? false);
        } catch {
          // Model loading failed, user can select model in settings
        }
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
        });
      } catch (err) {
        console.error("Failed to reload settings on settings:updated event:", err);
      }
    }));

    // Drop the parenthetical descriptor from a model display name (e.g.
    // "Large V3 Turbo (Best)" -> "Large V3 Turbo") so the toast inside the
    // 90×100 px overlay window stays readable on one or two lines.
    const shortModelName = (name: string): string => name.replace(/\s*\([^)]*\)\s*$/, "");

    // Language cycle shortcut: cycles through favoriteLanguages restricted to
    // those the CURRENT model supports. The model is sticky — only the model
    // shortcut changes it. If the current model has fewer than 2 supported
    // favorites, the cycle is a no-op with an explanatory toast.
    unlistenFns.push(await listen("shortcut:toggle-language", async () => {
      const favorites = store.settings.favoriteLanguages;
      if (favorites.length < 2) {
        store.showToggleNotification("Add 2+ favorite languages");
        return;
      }

      const modelLangs = store.settings.modelLanguages;
      const currentModel = store.settings.model;
      // Missing entry = "no restriction yet" → model accepts every favorite.
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
        // If we're already on that single eligible language, nothing to do.
        // Otherwise force-switch onto it so the user lands on a usable state.
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

      // Cycle within the eligible subset. If the current language is outside
      // the eligible list, jump to the first eligible entry instead of cycling
      // from a non-existent index.
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
