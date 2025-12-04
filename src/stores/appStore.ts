import { defineStore } from "pinia";
import { ref, computed } from "vue";

export type AppStatus = "idle" | "listening" | "processing" | "error";
export type ModelId = "small" | "large-v3-turbo";
export type Quantization = "q5_0" | "q5_1" | "q8_0";
export type Language = "auto" | "en" | "fr" | "es" | "de" | "it" | "pt" | "nl" | "ja" | "zh" | "ko" | "ar" | "hi" | "pl";

export interface ModelInfo {
  id: ModelId;
  name: string;
  size: string;
  downloaded: boolean;
  downloading: boolean;
  progress: number;
  bundled: boolean;
  availableQuants: Quantization[];  // Available quantizations on HuggingFace
}

export interface Settings {
  language: Language;
  model: ModelId;
  quantization: Quantization;
  autoCopy: boolean;
  shortcut: string;
}

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
  const showSettings = ref(false);
  const showPermissionGuide = ref(false);
  const permissionType = ref<"microphone" | null>(null);
  const showCopyNotification = ref(false);

  const settings = ref<Settings>({
    language: "auto",
    model: "large-v3-turbo",
    quantization: "q5_0",
    autoCopy: true,
    shortcut: "CommandOrControl+Shift+Space",
  });

  const permissions = ref<Permissions>({
    microphone: false,
  });

  // Bundled models only
  const models = ref<ModelInfo[]>([
    { id: "small", name: "Small (Fast)", size: "190 MB", downloaded: true, downloading: false, progress: 100, bundled: true, availableQuants: ["q5_0"] },
    { id: "large-v3-turbo", name: "Large V3 Turbo (Best)", size: "547 MB", downloaded: true, downloading: false, progress: 100, bundled: true, availableQuants: ["q5_0"] },
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

  function toggleSettings() {
    showSettings.value = !showSettings.value;
  }

  async function openPermissionGuide(type: "microphone") {
    permissionType.value = type;
    // Open a dedicated permission window instead of inline modal
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

  function closePermissionGuide() {
    showPermissionGuide.value = false;
    permissionType.value = null;
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

  // Get the best available quantization for a model
  // Prefers q5_0 > q5_1 > q8_0 (smaller = faster)
  function getBestQuantForModel(modelId: ModelId): Quantization {
    const model = models.value.find((m) => m.id === modelId);
    if (!model || model.availableQuants.length === 0) {
      return "q5_0"; // fallback
    }
    // Prefer q5_0 if available, then q5_1, then q8_0
    const preference: Quantization[] = ["q5_0", "q5_1", "q8_0"];
    for (const quant of preference) {
      if (model.availableQuants.includes(quant)) {
        return quant;
      }
    }
    return model.availableQuants[0];
  }

  return {
    // State
    status,
    vuLevel,
    partialTranscript,
    lastTranscript,
    showSettings,
    showPermissionGuide,
    permissionType,
    showCopyNotification,
    settings,
    permissions,
    models,
    history,
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
    toggleSettings,
    openPermissionGuide,
    closePermissionGuide,
    updateSettings,
    updateModelProgress,
    setModelDownloaded,
    setPermissions,
    addToHistory,
    setHistory,
    clearHistory,
    removeFromHistory,
    triggerCopyNotification,
    getBestQuantForModel,
  };
});
