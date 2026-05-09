import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  ALL_LANGUAGE_CODES,
  LANGUAGE_DISPLAY_NAMES as LANGUAGE_DISPLAY_NAMES_REGISTRY,
} from "../utils/languages";

export type AppStatus = "idle" | "listening" | "processing" | "error";
// `ModelId` was a closed union in v0.1.7 (only the two built-ins).
// Custom user-imported models use uuid-v4 ids so the type widens to
// `string`. The two built-in literals "small" / "large-v3-turbo" are
// still valid `ModelId` values; consumers that branch on them
// (welcome window, cycle shortcuts) continue to work unchanged.
export type ModelId = string;
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

/** Capabilities derived from a model's GGML header. Mirrors the Rust
 *  `ModelCapabilities` struct exposed by `whisper::compat`. */
export interface ModelCapabilities {
  isMultilingual: boolean;
  sizeClass: string;
  quantLabel: string;
  nVocab: number;
  nAudioState: number;
  nAudioLayer: number;
  fileSizeBytes: number;
}

export interface ModelInfo {
  id: ModelId;
  name: string;
  /** Human-friendly size shown in the picker, e.g. "190 MB". */
  size: string;
  /** Bytes from `capabilities.fileSizeBytes`. Used to rank models by
   *  capacity (proxy: the larger the .bin, the more capable the
   *  Whisper checkpoint) when language-first cycling needs to
   *  auto-pick a compatible model. */
  sizeBytes: number;
  downloaded: boolean;
  downloading: boolean;
  progress: number;
  bundled: boolean;
  /** `"builtin"` for the two registry models; `"custom"` for
   *  user-imported entries. Drives row-level UI (Delete icon only on
   *  custom, etc.). */
  kind: "builtin" | "custom";
  /** True when the user has marked this model as disabled. Cycle
   *  shortcuts skip it; the row renders muted. */
  disabled: boolean;
  /** Transient flag: the model failed to load this session. Cleared
   *  on app restart or via the Retry button. */
  broken: boolean;
  /** Reason returned by the backend when the load failed. */
  brokenReason?: string;
  /** Absolute path on disk for custom models. Built-ins resolve their
   *  path internally via `get_models_dir`. */
  path?: string;
  capabilities: ModelCapabilities;
}

/** Two-state behaviour switch for the language cycle shortcut.
 *  - `model-first`: cycle stays within favourites the active model supports
 *    (the v0.1.6 default). Use the model shortcut to swap model.
 *  - `language-first`: cycle through every favourite, auto-switching to the
 *    most capable compatible model when the current one can't transcribe
 *    the next language. */
export type LanguageCycleMode = "model-first" | "language-first";

/** Persisted shape of a user-imported Whisper model (Settings → Models
 *  Add flow). Mirrors the Rust `UserModel` struct. */
export interface UserModel {
  id: string;
  displayName: string;
  path: string;
  addedAt: number;
  capabilities: ModelCapabilities;
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
  /** Behaviour of the language cycle shortcut. See `LanguageCycleMode`. */
  languageCycleMode: LanguageCycleMode;
  /** User-imported Whisper models (in addition to the two built-ins).
   *  Empty for users who only use the bundled small / large-v3-turbo. */
  userModels: UserModel[];
  /** Model ids (built-in or custom) the user has marked as disabled.
   *  Disabled models are skipped by the cycle shortcuts. */
  disabledModels: string[];
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
    languageCycleMode: "model-first",
    userModels: [],
    disabledModels: [],
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

  // ---- Model download progress ------------------------------------------
  // Single source of truth for the in-flight model downloads. Driven by the
  // `model:download:progress|complete|error` Tauri events (the listeners
  // live in `useTauri.ts`). Used by:
  //   - MicButton.vue: ring + dim + click intercept while at least one model
  //     is missing or downloading.
  //   - SettingsPage.vue Models tab: per-row state machine (downloading /
  //     pending / failed → retry).
  //   - The cycle shortcut listeners: early-return with toast when downloads
  //     are in flight, instead of trying to load a `.partial` file.
  // Transient — never persisted (the desired state is "I want every required
  // model on disk", which we re-derive from disk on every launch via
  // `list_required_models`).
  interface ModelDownloadItem {
    id: string;
    displayName: string;
    sizeBytes: number;
    bytesReceived: number;
    percent: number;
    status: "pending" | "downloading" | "done" | "error";
    errorMessage?: string;
  }
  const modelDownload = ref<{
    /** True when at least one required model is not yet `done`. */
    active: boolean;
    items: ModelDownloadItem[];
  }>({ active: false, items: [] });

  /** Insert or merge a per-model download item. Used by the Tauri event
   *  listeners and by Retry handlers in Settings. */
  function upsertModelDownloadItem(
    id: string,
    patch: Partial<ModelDownloadItem> & { displayName?: string; sizeBytes?: number },
  ) {
    const existing = modelDownload.value.items.find((i) => i.id === id);
    if (existing) {
      Object.assign(existing, patch);
    } else {
      modelDownload.value.items.push({
        id,
        displayName: patch.displayName ?? id,
        sizeBytes: patch.sizeBytes ?? 0,
        bytesReceived: patch.bytesReceived ?? 0,
        percent: patch.percent ?? 0,
        status: patch.status ?? "pending",
        errorMessage: patch.errorMessage,
      });
    }
    recomputeModelDownloadActive();
  }

  /** Recompute the `active` flag whenever item statuses change. */
  function recomputeModelDownloadActive() {
    modelDownload.value.active = modelDownload.value.items.some(
      (i) => i.status !== "done",
    );
  }

  /** Reset the download state — useful when the app boots and no models are
   *  missing (clears any leftover from a previous session in the same store
   *  instance, which only happens in tests / hot reload). */
  function clearModelDownload() {
    modelDownload.value.items = [];
    modelDownload.value.active = false;
  }

  /** Cumulative percent across every tracked item. Used by the mic ring. */
  const modelDownloadCumulativePercent = computed(() => {
    const items = modelDownload.value.items;
    if (items.length === 0) return 0;
    const total = items.reduce((s, i) => s + i.sizeBytes, 0);
    if (total === 0) return 0;
    const done = items.reduce((s, i) => s + i.bytesReceived, 0);
    return Math.round((Math.min(done, total) / total) * 100);
  });

  // Models slice — seeded from the backend `list_all_models` command
  // on app boot (see `useTauri.ts initApp`). Starts empty so a window
  // that runs before init doesn't show stale built-in entries.
  // After init, contains the merged built-in + user-imported list
  // with capabilities, disabled state, etc.
  const models = ref<ModelInfo[]>([]);

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

  /** Replace the whole models list with the merged backend response.
   *  Called once on app boot (after `list_all_models`) and again
   *  whenever the import/disable/delete flows mutate the user-models
   *  list — both windows re-fetch via the existing `settings:updated`
   *  event so they stay in sync. */
  function setModels(list: ModelInfo[]) {
    models.value = list;
  }

  /** Toggle the disabled flag on a model id (built-in or custom).
   *  Optimistic local update; the matching backend command persists
   *  and broadcasts. */
  function setModelDisabledLocal(modelId: ModelId, disabled: boolean) {
    const model = models.value.find((m) => m.id === modelId);
    if (model) model.disabled = disabled;
  }

  /** Mark a model as broken with a reason — used by the runtime
   *  load-failure handler in `useTauri.loadWhisperModel`. */
  function markModelBroken(modelId: ModelId, reason: string) {
    const model = models.value.find((m) => m.id === modelId);
    if (model) {
      model.broken = true;
      model.brokenReason = reason;
    }
  }

  /** Clear the transient broken flag (used by the row-level Retry
   *  button before re-attempting the load). */
  function clearModelBroken(modelId: ModelId) {
    const model = models.value.find((m) => m.id === modelId);
    if (model) {
      model.broken = false;
      model.brokenReason = undefined;
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
    modelDownload,
    modelDownloadCumulativePercent,
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
    setModels,
    setModelDisabledLocal,
    markModelBroken,
    clearModelBroken,
    setPermissions,
    addToHistory,
    setHistory,
    clearHistory,
    removeFromHistory,
    triggerCopyNotification,
    showToggleNotification,
    upsertModelDownloadItem,
    clearModelDownload,
    // System health actions
    setSystemHealth,
    setGpuStatus,
    setVulkanWarningDismissed,
    openVulkanWarningModal,
    setWelcomeDismissed,
    openWelcomeModal,
  };
});
