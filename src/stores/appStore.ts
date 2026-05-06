import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  ALL_LANGUAGE_CODES,
  LANGUAGE_DISPLAY_NAMES as LANGUAGE_DISPLAY_NAMES_REGISTRY,
} from "../utils/languages";

export type AppStatus = "idle" | "listening" | "processing" | "error";
export type ModelId = "small" | "large-v3-turbo";
// `Language` used to be a 14-entry union; it's now any ISO 639-1 string
// the registry in `utils/languages.ts` accepts. Validation lives in Rust
// (`Language::is_known`) and in the registry — TS just keeps it loose.
export type Language = string;
export type GpuBackendType = "cpu" | "vulkan" | "metal" | "cuda" | "hipblas";

// System health check types
export interface SystemHealth {
  vulkanAvailable: boolean;
  vulkanVersion: string | null;
  gpuBackend: GpuBackendType;
  osInfo: {
    platform: string;
    version: string | null;
    distribution: string | null;
  };
  installGuide: VulkanInstallGuide | null;
  canRunWithoutVulkan: boolean;
}

export interface VulkanInstallGuide {
  title: string;
  description: string;
  steps: string[];
  downloadUrls: { name: string; url: string; description: string }[];
  terminalCommands: string[] | null;
}

export interface GpuStatus {
  usingGpu: boolean;
  backend: string;
  fallbackUsed: boolean;
}

export interface ModelInfo {
  id: ModelId;
  name: string;
  size: string;
  downloaded: boolean;
  downloading: boolean;
  progress: number;
  bundled: boolean;
}

export interface Settings {
  language: Language;
  model: ModelId;
  autoCopy: boolean;
  shortcut: string;
  /** Shortcut to cycle through favoriteLanguages. Empty = unbound. */
  languageToggleShortcut: string;
  /** Shortcut to cycle through models compatible with current language. Empty = unbound. */
  modelToggleShortcut: string;
  /** Languages cycled by the language shortcut. Order is the cycle order. */
  favoriteLanguages: Language[];
  /** Per-model language whitelist. Missing key = supports every favorite. */
  modelLanguages: Record<string, Language[]>;
}

// Re-exports kept for backward compat with components that already import
// these names. Source of truth is now `src/utils/languages.ts`.
export const ALL_LANGUAGES: Language[] = ALL_LANGUAGE_CODES;
export const LANGUAGE_DISPLAY_NAMES: Record<Language, string> = LANGUAGE_DISPLAY_NAMES_REGISTRY;

export interface Permissions {
  microphone: boolean;
}

export interface HistoryEntry {
  id: string;
  text: string;
  timestamp: number;
  modelId?: ModelId;
  durationMs?: number;
}

export const useAppStore = defineStore("app", () => {
  // State
  const status = ref<AppStatus>("idle");
  const vuLevel = ref(0);
  const partialTranscript = ref("");
  const lastTranscript = ref("");
  const showCopyNotification = ref(false);

  // Error toast state
  const errorMessage = ref<string | null>(null);
  const errorVisible = ref(false);

  // System health state
  const systemHealth = ref<SystemHealth | null>(null);
  const gpuStatus = ref<GpuStatus | null>(null);
  const vulkanWarningDismissed = ref(false);
  const welcomeDismissed = ref(false);

  const settings = ref<Settings>({
    language: "auto",
    model: "large-v3-turbo",
    autoCopy: true,
    shortcut: "CommandOrControl+Shift+Space",
    languageToggleShortcut: "",
    modelToggleShortcut: "",
    favoriteLanguages: [...ALL_LANGUAGES],
    modelLanguages: {},
  });

  // Toast shown above the mic button after a language/model toggle.
  const toggleNotification = ref<{ text: string; visible: boolean }>({ text: "", visible: false });
  let toggleNotificationTimer: ReturnType<typeof setTimeout> | null = null;

  function showToggleNotification(text: string) {
    if (toggleNotificationTimer) clearTimeout(toggleNotificationTimer);
    toggleNotification.value = { text, visible: true };
    toggleNotificationTimer = setTimeout(() => {
      toggleNotification.value = { text: "", visible: false };
      toggleNotificationTimer = null;
    }, 1500);
  }

  const permissions = ref<Permissions>({
    microphone: false,
  });

  // Bundled models only
  const models = ref<ModelInfo[]>([
    { id: "small", name: "Small (Fast)", size: "190 MB", downloaded: true, downloading: false, progress: 100, bundled: true },
    { id: "large-v3-turbo", name: "Large V3 Turbo (Best)", size: "547 MB", downloaded: true, downloading: false, progress: 100, bundled: true },
  ]);

  const history = ref<HistoryEntry[]>([]);
  const MAX_HISTORY = 20;

  // Computed
  const isListening = computed(() => status.value === "listening");
  const isProcessing = computed(() => status.value === "processing");
  const hasError = computed(() => status.value === "error");
  const currentModel = computed(() => models.value.find((m) => m.id === settings.value.model));

  // Actions
  function setStatus(newStatus: AppStatus) {
    status.value = newStatus;
  }

  function setVuLevel(level: number) {
    vuLevel.value = Math.max(0, Math.min(1, level));
  }

  function setPartialTranscript(text: string) {
    partialTranscript.value = text;
  }

  function setLastTranscript(text: string) {
    lastTranscript.value = text;
    partialTranscript.value = "";
  }

  // Error toast actions
  function showError(message: string) {
    errorMessage.value = message;
    errorVisible.value = true;
    setTimeout(() => {
      errorVisible.value = false;
      errorMessage.value = null;
    }, 5000);
  }

  function clearError() {
    errorMessage.value = null;
    errorVisible.value = false;
  }

  async function openPermissionGuide(type: "microphone") {
    // Open a dedicated permission window
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");

    // Check if permission window already exists
    const existingWindow = await WebviewWindow.getByLabel("permissions");
    if (existingWindow) {
      await existingWindow.setFocus();
      return;
    }

    // Create new permission window
    const permWindow = new WebviewWindow("permissions", {
      url: `/permissions.html?type=${type}`,
      title: "Permissions - S2Tui",
      width: 480,
      height: 420,
      minWidth: 400,
      minHeight: 350,
      resizable: false,
      center: true,
      decorations: false,
      transparent: false,
      shadow: true,
      alwaysOnTop: true,
    });

    permWindow.once("tauri://error", (e) => {
      console.error("Failed to create permissions window:", e);
    });
  }

  function updateSettings(newSettings: Partial<Settings>) {
    settings.value = { ...settings.value, ...newSettings };
  }

  function updateModelProgress(modelId: ModelId, progress: number, downloading: boolean) {
    const model = models.value.find((m) => m.id === modelId);
    if (model) {
      model.progress = progress;
      model.downloading = downloading;
      if (progress >= 100) {
        model.downloaded = true;
        model.downloading = false;
      }
    }
  }

  function setModelDownloaded(modelId: ModelId, downloaded: boolean) {
    const model = models.value.find((m) => m.id === modelId);
    if (model) {
      model.downloaded = downloaded;
      model.progress = downloaded ? 100 : 0;
    }
  }

  function setPermissions(perms: Partial<Permissions>) {
    permissions.value = { ...permissions.value, ...perms };
  }

  function addToHistory(text: string, modelId?: ModelId, durationMs?: number) {
    const entry: HistoryEntry = {
      id: Date.now().toString(),
      text,
      timestamp: Date.now(),
      modelId,
      durationMs,
    };
    history.value.unshift(entry);
    if (history.value.length > MAX_HISTORY) {
      history.value = history.value.slice(0, MAX_HISTORY);
    }
  }

  function setHistory(entries: HistoryEntry[]) {
    history.value = entries.slice(0, MAX_HISTORY);
  }

  function clearHistory() {
    history.value = [];
  }

  function removeFromHistory(id: string) {
    history.value = history.value.filter((entry) => entry.id !== id);
  }

  function triggerCopyNotification() {
    showCopyNotification.value = true;
    setTimeout(() => {
      showCopyNotification.value = false;
    }, 2000);
  }

  // System health actions
  function setSystemHealth(health: SystemHealth) {
    systemHealth.value = health;
  }

  function setGpuStatus(status: GpuStatus) {
    gpuStatus.value = status;
  }

  function setVulkanWarningDismissed(dismissed: boolean) {
    vulkanWarningDismissed.value = dismissed;
  }

  async function openVulkanWarningModal() {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");

    // Check if window already exists
    const existingWindow = await WebviewWindow.getByLabel("vulkan-warning");
    if (existingWindow) {
      await existingWindow.setFocus();
      return;
    }

    // Create new vulkan warning window with native decorations
    const warningWindow = new WebviewWindow("vulkan-warning", {
      url: "vulkan-warning.html",
      title: "GPU Acceleration - S2Tui",
      width: 520,
      height: 580,
      minWidth: 450,
      minHeight: 450,
      resizable: true,
      center: true,
      decorations: true,
      transparent: false,
      shadow: true,
      alwaysOnTop: false,
    });

    warningWindow.once("tauri://error", (e) => {
      console.error("Failed to create vulkan warning window:", e);
    });
  }

  function setWelcomeDismissed(dismissed: boolean) {
    welcomeDismissed.value = dismissed;
  }

  async function openWelcomeModal() {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");

    // Check if window already exists
    const existingWindow = await WebviewWindow.getByLabel("welcome");
    if (existingWindow) {
      await existingWindow.setFocus();
      return;
    }

    // Create new welcome window
    const welcomeWindow = new WebviewWindow("welcome", {
      url: "welcome.html",
      title: "Welcome - S2Tui",
      width: 550,
      height: 750,
      minWidth: 450,
      minHeight: 650,
      resizable: false,
      center: true,
      decorations: false,
      transparent: false,
      shadow: true,
      alwaysOnTop: false,
    });

    welcomeWindow.once("tauri://error", (e) => {
      console.error("Failed to create welcome window:", e);
    });
  }

  return {
    // State
    status,
    vuLevel,
    partialTranscript,
    lastTranscript,
    showCopyNotification,
    toggleNotification,
    errorMessage,
    errorVisible,
    settings,
    permissions,
    models,
    history,
    // System health state
    systemHealth,
    gpuStatus,
    vulkanWarningDismissed,
    welcomeDismissed,
    // Computed
    isListening,
    isProcessing,
    hasError,
    currentModel,
    // Actions
    setStatus,
    setVuLevel,
    setPartialTranscript,
    setLastTranscript,
    showError,
    clearError,
    openPermissionGuide,
    updateSettings,
    updateModelProgress,
    setModelDownloaded,
    setPermissions,
    addToHistory,
    setHistory,
    clearHistory,
    removeFromHistory,
    triggerCopyNotification,
    showToggleNotification,
    // System health actions
    setSystemHealth,
    setGpuStatus,
    setVulkanWarningDismissed,
    openVulkanWarningModal,
    setWelcomeDismissed,
    openWelcomeModal,
  };
});
