import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useAppStore, type ModelId, type Language, type SystemHealth, type GpuStatus } from "../stores/appStore";
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

  // Commands - Settings
  async function setModel(name: ModelId) {
    try {
      await invoke("set_model", { name });
    } catch (error) {
      console.error("Failed to set model:", error);
    }
  }

  async function setLanguage(lang: Language) {
    try {
      await invoke("set_language", { lang });
    } catch (error) {
      console.error("Failed to set language:", error);
    }
  }

  async function setShortcut(shortcut: string): Promise<void> {
    try {
      await invoke("set_shortcut", { shortcut });
      store.updateSettings({ shortcut });
      await saveSettings({ shortcut });
    } catch (error) {
      console.error("Failed to set shortcut:", error);
      throw error;
    }
  }

  // Commands - Model Management
  async function loadWhisperModel(model: ModelId) {
    try {
      await invoke("load_whisper_model", { model });
      store.updateSettings({ model });
      await saveSettings({ model });
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
      });

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

        // Persist history in background (don't block copy notification)
        addHistoryEntry(text, model, transcribeDurationMs).catch((error) => {
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
