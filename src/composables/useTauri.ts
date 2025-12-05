import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useAppStore, type ModelId, type Language } from "../stores/appStore";
import { loadSettings, saveSettings, addHistoryEntry, loadHistory } from "./useStore";

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
      console.log("Loading whisper model:", model);
      await invoke("load_whisper_model", { model });
      store.updateSettings({ model });
      await saveSettings({ model });
      console.log("Model saved to settings:", model);
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
      const models = await invoke<string[]>("get_available_models");
      console.log("Available models on disk:", models);
      return models;
    } catch (error) {
      console.error("Failed to get available models:", error);
      return [];
    }
  }

  // Load persisted settings and initialize app
  async function initApp() {
    try {
      // Load persisted settings
      const persisted = await loadSettings();
      console.log("Loaded persisted settings:", persisted);

      // Update store with persisted settings
      store.updateSettings({
        language: persisted.language,
        model: persisted.model,
        autoCopy: persisted.autoCopy,
        shortcut: persisted.shortcut,
      });

      // Detect actually available models on disk (not just persisted state)
      const availableModels = await getAvailableModels();
      console.log("Available models on disk:", availableModels);

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

      // Load the whisper model if available
      if (availableModels.includes(persisted.model)) {
        try {
          console.log("Loading persisted model:", persisted.model);
          await loadWhisperModel(persisted.model);
          console.log("Whisper model loaded successfully:", persisted.model);
        } catch (error) {
          console.warn("Could not load whisper model:", error);
        }
      } else {
        console.warn("Selected model not available on disk:", persisted.model, "Available:", availableModels);
      }
    } catch (error) {
      console.error("Failed to initialize app:", error);
    }
  }

  // Initialize event listeners
  async function initListeners() {
    // Prevent duplicate initialization
    if (listenersInitialized) {
      console.log("[initListeners] Already initialized, skipping");
      return;
    }
    listenersInitialized = true;
    console.log("[initListeners] Initializing listeners...");

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
      console.log("[permission:granted] Received:", event.payload);
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

    // Global shortcut listener - with guard to prevent duplicate actions
    unlistenFns.push(await listen("shortcut:triggered", async () => {
      // Prevent duplicate actions if already processing
      if (isActionInProgress) {
        console.log("[shortcut] Action already in progress, ignoring");
        return;
      }

      console.log("[shortcut] Triggered, status:", store.status);

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
    isModelLoaded,
    getAvailableModels,
    // Permissions
    checkPermissions,
    requestMicrophonePermission,
    // Init
    initListeners,
    initApp,
  };
}
