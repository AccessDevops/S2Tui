<script setup lang="ts">
import { computed, nextTick, ref, watch, onMounted, onUnmounted } from "vue";
import {
  useAppStore,
  type Language,
  type ModelId,
  ALL_LANGUAGES,
  LANGUAGE_DISPLAY_NAMES,
} from "../stores/appStore";
import { displayNameFor, tierFor } from "../utils/languages";
import { flagUrlFor } from "../utils/flags";
import { useTauri } from "../composables/useTauri";
import { loadSettings, saveSettings, loadHistory, clearHistory as clearHistoryStore } from "../composables/useStore";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ShortcutCapture from "../components/ShortcutCapture.vue";

const store = useAppStore();
const {
  setLanguage,
  setShortcut,
  setLanguageToggleShortcut,
  setModelToggleShortcut,
  setFavoriteLanguages,
  setModelLanguages,
  loadWhisperModel,
  checkSystemHealth,
  checkPermissions,
} = useTauri();

const shortcutError = ref<string | null>(null);
const langToggleShortcutError = ref<string | null>(null);
const modelToggleShortcutError = ref<string | null>(null);

// App version pulled at runtime from tauri.conf.json so the footer never
// drifts from the actually-built binary's version (the previous hard-coded
// "v0.1.0" was visibly stale after every release).
const appVersion = ref<string>("");

const settings = computed(() => store.settings);
const models = computed(() => store.models);
const permissions = computed(() => store.permissions);
const history = computed(() => store.history);
const systemHealth = computed(() => store.systemHealth);
const gpuStatus = computed(() => store.gpuStatus);

// Each ShortcutCapture sees the *other* two registered shortcuts and rejects
// duplicates during capture so two actions never share the same key combo.
const mainShortcutConflicts = computed(() =>
  [settings.value.languageToggleShortcut, settings.value.modelToggleShortcut].filter(Boolean),
);
const langToggleConflicts = computed(() =>
  [settings.value.shortcut, settings.value.modelToggleShortcut].filter(Boolean),
);
const modelToggleConflicts = computed(() =>
  [settings.value.shortcut, settings.value.languageToggleShortcut].filter(Boolean),
);

// Models the user has actually downloaded — used for the per-model whitelist UI.
const downloadedModels = computed(() => models.value.filter((m) => m.downloaded));

function modelLanguagesFor(modelId: string): Language[] {
  const list = settings.value.modelLanguages[modelId];
  // Undefined = no override yet, behave like "supports every favorite".
  return list ?? [...settings.value.favoriteLanguages];
}

const activeTab = ref<"general" | "models" | "permissions" | "history" | "system">("general");
const copiedId = ref<string | null>(null);
const loadingModelId = ref<ModelId | null>(null);
let unlistenHistory: UnlistenFn | null = null;
let unlistenSettingsUpdated: UnlistenFn | null = null;

// Pull latest settings from persistence into the local Pinia store. Used both
// for the initial load on mount and as the handler for `settings:updated`
// events emitted by the main window (toggle shortcut, etc.).
async function syncFromPersistence() {
  try {
    const persisted = await loadSettings();
    // `auto` must always be a favourite — it's the auto-detect sentinel.
    // Self-heal here for any settings.json file that's missing it (e.g.
    // saved with an earlier build that allowed removing auto).
    const persistedFavs = persisted.favoriteLanguages ?? [...ALL_LANGUAGES];
    const favs = persistedFavs.includes("auto") ? persistedFavs : ["auto", ...persistedFavs];
    store.updateSettings({
      language: persisted.language,
      model: persisted.model,
      autoCopy: persisted.autoCopy,
      shortcut: persisted.shortcut,
      languageToggleShortcut: persisted.languageToggleShortcut ?? "",
      modelToggleShortcut: persisted.modelToggleShortcut ?? "",
      favoriteLanguages: favs,
      modelLanguages: persisted.modelLanguages ?? {},
    });
    if (favs !== persistedFavs) {
      // Persist the corrected list so the next launch starts clean.
      await setFavoriteLanguages(favs);
    }
  } catch (e) {
    console.error("Failed to load persisted settings:", e);
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    closeWindow();
  }
}

onMounted(async () => {
  // Note: Do NOT call initListeners() here - it triggers initApp() which would
  // re-open the welcome modal. Settings window has its own Pinia context (not shared).

  // Settings window has its own Pinia context, so the persisted toggle config
  // (favorite languages, per-model whitelists, shortcuts) must be loaded here.
  await syncFromPersistence();

  // Load history from persistence
  const savedHistory = await loadHistory();
  store.setHistory(savedHistory);

  // Listen for history updates from other windows (e.g., main window after transcription)
  unlistenHistory = await listen("history:updated", async () => {
    // Reload history from persistence when notified of updates
    const updatedHistory = await loadHistory();
    store.setHistory(updatedHistory);
  });

  // Listen for cross-window settings mutations (toggle shortcuts in the main
  // window, future tray actions, …) so this UI reflects them live without
  // requiring the user to close and reopen the Settings window.
  unlistenSettingsUpdated = await listen("settings:updated", async () => {
    await syncFromPersistence();
  });

  // Check permissions for the Permissions tab
  try {
    await checkPermissions();
  } catch (error) {
    console.error("Failed to check permissions:", error);
  }

  // Load system health info for the System tab
  try {
    await checkSystemHealth();
  } catch (error) {
    console.error("Failed to load system health:", error);
  }

  // Pull the actual app version from Tauri (reads tauri.conf.json) so the
  // footer matches the built binary instead of a stale hard-coded literal.
  try {
    const { getVersion } = await import("@tauri-apps/api/app");
    appVersion.value = await getVersion();
  } catch (error) {
    console.error("Failed to read app version:", error);
  }

  // Listen for Escape key to close window
  window.addEventListener("keydown", handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener("keydown", handleKeydown);

  // Clean up event listeners
  if (unlistenHistory) unlistenHistory();
  if (unlistenSettingsUpdated) unlistenSettingsUpdated();
});

// History handlers
async function copyToClipboard(text: string, id: string) {
  const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
  await writeText(text);
  copiedId.value = id;
  setTimeout(() => {
    copiedId.value = null;
  }, 2000);
}

async function handleClearHistory() {
  store.clearHistory();
  await clearHistoryStore();
}

function formatDate(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - date.getTime();

  if (diff < 60000) return "Just now";
  if (diff < 3600000) return `${Math.floor(diff / 60000)} min ago`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} h ago`;

  return date.toLocaleDateString("en-US", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatDuration(ms: number | undefined): string {
  if (!ms) return "";
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function getShortModelName(modelId: string | undefined): string {
  if (!modelId) return "";
  const names: Record<string, string> = {
    "base": "Base",
    "small": "Small",
    "medium": "Medium",
    "large-v3": "Large",
    "large-v3-turbo": "Turbo",
  };
  return names[modelId] || modelId;
}

// Settings handlers
async function handleLanguageChange(e: Event) {
  const lang = (e.target as HTMLSelectElement).value as Language;
  // setLanguage now persists and broadcasts on its own.
  await setLanguage(lang);
}

async function handleAutoCopyChange(e: Event) {
  const enabled = (e.target as HTMLInputElement).checked;
  store.updateSettings({ autoCopy: enabled });
  await saveSettings({ autoCopy: enabled });
}

async function handleShortcutChange(newShortcut: string) {
  shortcutError.value = null;
  try {
    await setShortcut(newShortcut);
  } catch (error) {
    shortcutError.value = error instanceof Error ? error.message : "Unable to register this shortcut. It may already be used by another application.";
    // Revert to previous shortcut in UI
    store.updateSettings({ shortcut: settings.value.shortcut });
  }
}

async function handleLanguageToggleShortcutChange(newShortcut: string) {
  langToggleShortcutError.value = null;
  try {
    await setLanguageToggleShortcut(newShortcut);
  } catch (error) {
    langToggleShortcutError.value =
      error instanceof Error ? error.message : "Unable to register this shortcut.";
    store.updateSettings({ languageToggleShortcut: settings.value.languageToggleShortcut });
  }
}

async function handleModelToggleShortcutChange(newShortcut: string) {
  modelToggleShortcutError.value = null;
  try {
    await setModelToggleShortcut(newShortcut);
  } catch (error) {
    modelToggleShortcutError.value =
      error instanceof Error ? error.message : "Unable to register this shortcut.";
    store.updateSettings({ modelToggleShortcut: settings.value.modelToggleShortcut });
  }
}

async function removeFavoriteLanguage(lang: Language) {
  // `auto` is the auto-detect sentinel — keep it pinned in favourites so
  // the cycle shortcut always has at least one universally-usable option,
  // and so users can always fall back to auto-detect via the picker.
  if (lang === "auto") return;
  const next = settings.value.favoriteLanguages.filter((l) => l !== lang);
  await setFavoriteLanguages(next);
}

async function clearFavoriteLanguages() {
  // Preserve `auto` for the same reason as above.
  await setFavoriteLanguages(["auto"]);
}

// ---- "Add language" popover ----------------------------------------------
// Replaces the legacy giant 2-column checklist. Keeps the picker compact
// even with 60+ languages: chips show what's selected, an inline search
// dropdown adds new ones one by one.
const addPickerOpen = ref(false);
const addPickerSearch = ref("");
const addPickerWrapperRef = ref<HTMLElement | null>(null);
const addPickerSearchInputRef = ref<HTMLInputElement | null>(null);

// Diacritic-insensitive lowercase: "francais" matches "Français",
// "lv" matches Latviešu by code, "中" matches 中文 directly.
function normalizeForSearch(s: string): string {
  return s.toLowerCase().normalize("NFD").replace(/\p{Diacritic}/gu, "");
}

function languageMatchesSearch(code: string, query: string): boolean {
  if (!query) return true;
  const q = normalizeForSearch(query);
  if (code.toLowerCase().includes(q)) return true;
  return normalizeForSearch(displayNameFor(code)).includes(q);
}

const addableLanguages = computed<Language[]>(() => {
  const selected = new Set(settings.value.favoriteLanguages);
  return ALL_LANGUAGES.filter((code) => code !== "auto" && !selected.has(code));
});

const filteredAddableLanguages = computed<Language[]>(() => {
  return addableLanguages.value.filter((code) =>
    languageMatchesSearch(code, addPickerSearch.value),
  );
});

async function addFavoriteLanguage(lang: Language) {
  if (settings.value.favoriteLanguages.includes(lang)) return;
  const next = [...settings.value.favoriteLanguages, lang];
  await setFavoriteLanguages(next);
  addPickerSearch.value = "";
  // Keep the popover open so the user can chain multiple adds.
  // Re-focus the search so they can keep typing.
  nextTick(() => addPickerSearchInputRef.value?.focus());
}

function handleAddPickerOutsideClick(e: MouseEvent) {
  if (
    addPickerWrapperRef.value &&
    !addPickerWrapperRef.value.contains(e.target as Node)
  ) {
    addPickerOpen.value = false;
  }
}

function handleAddPickerKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    addPickerOpen.value = false;
  } else if (e.key === "Enter" && filteredAddableLanguages.value.length > 0) {
    e.preventDefault();
    addFavoriteLanguage(filteredAddableLanguages.value[0]);
  }
}

watch(addPickerOpen, (open) => {
  if (open) {
    document.addEventListener("mousedown", handleAddPickerOutsideClick);
    nextTick(() => addPickerSearchInputRef.value?.focus());
  } else {
    document.removeEventListener("mousedown", handleAddPickerOutsideClick);
    addPickerSearch.value = "";
  }
});

async function handleModelLanguageToggle(modelId: string, lang: Language) {
  const current = modelLanguagesFor(modelId);
  const next = [...current];
  const index = next.indexOf(lang);
  if (index >= 0) next.splice(index, 1);
  else next.push(lang);
  await setModelLanguages(modelId, next);
}

// Model handlers
async function handleSelectModel(modelId: ModelId) {
  const model = models.value.find((m) => m.id === modelId);
  if (model && model.downloaded) {
    loadingModelId.value = modelId;
    try {
      await loadWhisperModel(modelId);
    } catch (error) {
      console.error("Failed to load model:", error);
      store.showError(`Failed to load model: ${error instanceof Error ? error.message : "Unknown error"}`);
    } finally {
      loadingModelId.value = null;
    }
  }
}

async function closeWindow() {
  const window = getCurrentWebviewWindow();
  await window.close();
}

// System handlers
async function refreshSystemHealth() {
  try {
    await checkSystemHealth();
  } catch (error) {
    console.error("Failed to refresh system health:", error);
  }
}

function getBackendColor(backend: string | undefined): string {
  if (!backend) return "bg-gray-500/20 text-gray-400";
  switch (backend.toLowerCase()) {
    case "vulkan":
      return "bg-green-500/20 text-green-400";
    case "metal":
      return "bg-green-500/20 text-green-400";
    case "cuda":
      return "bg-green-500/20 text-green-400";
    case "cpu":
      return "bg-amber-500/20 text-amber-400";
    default:
      return "bg-gray-500/20 text-gray-400";
  }
}
</script>

<template>
  <div class="min-h-screen bg-gradient-to-br from-gray-900 via-gray-800 to-gray-900">
    <!-- Title bar drag region -->
    <div data-tauri-drag-region class="h-8 flex items-center justify-between px-4 bg-black/20">
      <span class="text-white/60 text-sm font-medium">S2Tui Settings</span>
      <button
        @click="closeWindow"
        class="p-1 rounded hover:bg-white/10 text-white/60 hover:text-white transition-colors"
      >
        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <div class="flex h-[calc(100vh-32px)]">
      <!-- Sidebar -->
      <div class="w-56 bg-black/20 border-r border-white/10 p-4 flex flex-col">
        <nav class="space-y-1 flex-1">
          <button
            v-for="tab in [
              { id: 'general', label: 'General', icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z' },
              { id: 'models', label: 'Whisper Models', icon: 'M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10' },
              { id: 'history', label: 'History', icon: 'M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z' },
              { id: 'system', label: 'System', icon: 'M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z' },
              { id: 'permissions', label: 'Permissions', icon: 'M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z' }
            ]"
            :key="tab.id"
            @click="activeTab = tab.id as any"
            :class="[
              'w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all',
              activeTab === tab.id
                ? 'bg-mic-listening text-white shadow-lg'
                : 'text-white/70 hover:text-white hover:bg-white/10'
            ]"
          >
            <svg class="w-5 h-5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="tab.icon" />
            </svg>
            {{ tab.label }}
          </button>
        </nav>

        <!-- App info -->
        <div class="text-center pt-4 space-y-1">
          <p class="text-white/30 text-xs">S2Tui {{ appVersion ? `v${appVersion}` : '' }}</p>
          <p class="text-white/30 text-xs">
            made with ❤️ by
            <a
              href="https://cbarange.com"
              target="_blank"
              class="text-white/50 hover:text-white transition-colors underline"
            >cbarange</a>
          </p>
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 p-8 overflow-y-auto">
        <!-- General Tab -->
        <div v-if="activeTab === 'general'" class="max-w-xl space-y-8">
          <div>
            <h2 class="text-white text-xl font-semibold mb-1">General Settings</h2>
            <p class="text-white/50 text-sm">Configure the application behavior</p>
          </div>

          <!-- Language -->
          <div class="space-y-2">
            <label class="block text-white font-medium">Transcription Language</label>
            <p class="text-white/50 text-sm">Select the language for speech recognition</p>
            <select
              :value="settings.language"
              @change="handleLanguageChange"
              class="w-full bg-white/10 text-white rounded-lg px-4 py-3 border border-white/20 focus:outline-none focus:border-mic-listening focus:ring-1 focus:ring-mic-listening transition-all"
            >
              <option value="auto" class="bg-gray-800">Auto-detect</option>
              <option value="fr" class="bg-gray-800">Français</option>
              <option value="en" class="bg-gray-800">English</option>
              <option value="es" class="bg-gray-800">Español</option>
              <option value="de" class="bg-gray-800">Deutsch</option>
              <option value="it" class="bg-gray-800">Italiano</option>
              <option value="pt" class="bg-gray-800">Português</option>
              <option value="nl" class="bg-gray-800">Nederlands</option>
              <option value="ja" class="bg-gray-800">日本語</option>
              <option value="zh" class="bg-gray-800">中文</option>
              <option value="ko" class="bg-gray-800">한국어</option>
              <option value="ar" class="bg-gray-800">العربية</option>
              <option value="hi" class="bg-gray-800">हिन्दी</option>
              <option value="pl" class="bg-gray-800">Polski</option>
            </select>
          </div>

          <!-- Auto-copy toggle -->
          <div class="flex items-center justify-between p-4 bg-white/5 rounded-xl border border-white/10">
            <div>
              <p class="text-white font-medium">Copy to clipboard</p>
              <p class="text-white/50 text-sm mt-1">
                {{ settings.autoCopy ? "Transcribed text is automatically copied to clipboard" : "Text is not automatically copied" }}
              </p>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                :checked="settings.autoCopy"
                @change="handleAutoCopyChange"
                class="sr-only peer"
              />
              <div class="w-14 h-7 bg-white/20 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-6 after:w-6 after:transition-all peer-checked:bg-mic-listening"></div>
            </label>
          </div>

          <!-- Shortcut configuration -->
          <div class="space-y-2">
            <label class="block text-white font-medium">Keyboard shortcut</label>
            <p class="text-white/50 text-sm">Press this shortcut to start/stop listening</p>
            <ShortcutCapture
              :model-value="settings.shortcut"
              :conflict-shortcuts="mainShortcutConflicts"
              @change="handleShortcutChange"
            />
            <p v-if="shortcutError" class="text-red-400 text-sm flex items-center gap-2">
              <svg class="w-4 h-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {{ shortcutError }}
            </p>
          </div>

          <!-- Model cycle shortcut -->
          <div class="space-y-2">
            <label class="block text-white font-medium">Model cycle shortcut</label>
            <p class="text-white/50 text-sm">Press this shortcut to cycle through models compatible with the current language (reloads model — may take a few seconds)</p>
            <ShortcutCapture
              :model-value="settings.modelToggleShortcut"
              :conflict-shortcuts="modelToggleConflicts"
              :clearable="true"
              @change="handleModelToggleShortcutChange"
            />
            <p v-if="modelToggleShortcutError" class="text-red-400 text-sm flex items-center gap-2">
              <svg class="w-4 h-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {{ modelToggleShortcutError }}
            </p>
          </div>

          <!-- Language cycle shortcut -->
          <div class="space-y-2">
            <label class="block text-white font-medium">Language cycle shortcut</label>
            <p class="text-white/50 text-sm">Press this shortcut to cycle through your favorite languages</p>
            <ShortcutCapture
              :model-value="settings.languageToggleShortcut"
              :conflict-shortcuts="langToggleConflicts"
              :clearable="true"
              @change="handleLanguageToggleShortcutChange"
            />
            <p v-if="langToggleShortcutError" class="text-red-400 text-sm flex items-center gap-2">
              <svg class="w-4 h-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {{ langToggleShortcutError }}
            </p>
          </div>

          <!-- Favorite languages: chips for selection + searchable Add picker.
               Replaces the legacy 60-row checklist now that the supported
               language set has grown well past what fits on screen. -->
          <div class="space-y-3">
            <div>
              <label class="block text-white font-medium">Favorite languages</label>
              <p class="text-white/50 text-sm">Languages cycled by the shortcut. Click + Add or remove with the ✕. Auto stays pinned.</p>
            </div>

            <!-- Action row: Add picker + Clear all on the same line, above the chips -->
            <div class="flex items-center gap-2">
              <div class="relative inline-block" ref="addPickerWrapperRef">
                <button
                  @click="addPickerOpen = !addPickerOpen"
                  :class="[
                    'inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs transition-colors',
                    addPickerOpen
                      ? 'bg-mic-listening/30 border border-mic-listening/60 text-white'
                      : 'bg-white/10 hover:bg-white/15 border border-white/10 text-white/80 hover:text-white',
                  ]"
                >
                  <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
                  </svg>
                  Add language
                </button>

              <Transition
                enter-active-class="transition-all duration-150 ease-out"
                enter-from-class="opacity-0 -translate-y-1"
                enter-to-class="opacity-100 translate-y-0"
                leave-active-class="transition-all duration-100 ease-in"
                leave-from-class="opacity-100 translate-y-0"
                leave-to-class="opacity-0 -translate-y-1"
              >
                <div
                  v-if="addPickerOpen"
                  class="absolute left-0 top-full mt-2 z-30 w-72 rounded-lg bg-gray-900/95 border border-white/15 shadow-xl backdrop-blur-sm overflow-hidden"
                >
                  <input
                    ref="addPickerSearchInputRef"
                    v-model="addPickerSearch"
                    @keydown="handleAddPickerKeydown"
                    type="text"
                    placeholder="Search languages…"
                    class="w-full px-3 py-2 bg-transparent border-b border-white/10 text-white text-sm placeholder-white/30 focus:outline-none"
                  />
                  <div class="max-h-72 overflow-y-auto py-1">
                    <button
                      v-for="code in filteredAddableLanguages"
                      :key="`add-${code}`"
                      @click="addFavoriteLanguage(code)"
                      class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-white/10 transition-colors"
                    >
                      <img
                        v-if="flagUrlFor(code)"
                        :src="flagUrlFor(code)"
                        :alt="''"
                        class="w-4 h-4 rounded-full flex-shrink-0"
                      />
                      <span class="w-4 h-4 rounded-full bg-white/10 flex-shrink-0" v-else></span>
                      <span class="text-white text-sm flex-1 truncate">{{ displayNameFor(code) }}</span>
                      <span
                        v-if="tierFor(code) === 'medium'"
                        class="text-amber-300/70 text-[10px]"
                      >medium</span>
                      <span class="text-white/30 text-[10px] font-mono">{{ code }}</span>
                    </button>
                    <div
                      v-if="filteredAddableLanguages.length === 0"
                      class="px-3 py-3 text-white/40 text-sm text-center"
                    >
                      <span v-if="addableLanguages.length === 0">All languages added</span>
                      <span v-else>No language matches "{{ addPickerSearch }}"</span>
                    </div>
                  </div>
                </div>
              </Transition>
              </div>

              <!-- Clear all sits beside the Add button on the same line. -->
              <button
                v-if="settings.favoriteLanguages.length > 1"
                @click="clearFavoriteLanguages"
                class="px-3 py-1.5 rounded-md text-xs bg-white/10 hover:bg-red-500/30 border border-white/10 text-white/60 hover:text-white transition-colors"
              >Clear all</button>
            </div>

            <!-- Selected favorites as chips, below the action row -->
            <div class="flex flex-wrap gap-1.5">
              <span
                v-for="code in settings.favoriteLanguages"
                :key="`fav-${code}`"
                class="inline-flex items-center gap-1.5 pl-1.5 pr-1 py-1 rounded-md bg-white/10 border border-white/10 text-white text-xs"
              >
                <img
                  v-if="flagUrlFor(code)"
                  :src="flagUrlFor(code)"
                  :alt="''"
                  class="w-4 h-4 rounded-full flex-shrink-0"
                />
                <span class="w-4 h-4 rounded-full bg-white/15 flex-shrink-0" v-else></span>
                <span>{{ displayNameFor(code) }}</span>
                <span
                  v-if="tierFor(code) === 'medium'"
                  class="text-amber-300/70 text-[10px]"
                  title="Medium-quality language (100–1000 h Whisper training data)"
                >·m</span>
                <button
                  v-if="code !== 'auto'"
                  @click="removeFavoriteLanguage(code)"
                  class="ml-0.5 w-4 h-4 rounded-full hover:bg-red-500/40 text-white/40 hover:text-white flex items-center justify-center transition-colors"
                  :title="`Remove ${displayNameFor(code)}`"
                >
                  <svg class="w-2.5 h-2.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
                <span v-else class="ml-0.5 w-4 h-4" aria-hidden="true"></span>
              </span>
            </div>

            <!-- The min-2 hint is only relevant when the user actually
                 bound a language cycle shortcut — otherwise the favourites
                 list is just a passive picker, no minimum required. -->
            <p
              v-if="settings.languageToggleShortcut && settings.favoriteLanguages.length < 2"
              class="text-amber-400 text-sm flex items-center gap-2"
            >
              <svg class="w-4 h-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              The language cycle shortcut needs at least 2 favourites to do anything.
            </p>
          </div>

          <!-- Per-model language matrix -->
          <div v-if="downloadedModels.length > 0" class="space-y-3">
            <div>
              <label class="block text-white font-medium">Languages per model</label>
              <p class="text-white/50 text-sm">
                Restrict each model to the languages it transcribes well. The language shortcut only cycles within
                the languages enabled on the currently selected model — it never changes the model.
                Unconfigured models accept every favorite language.
              </p>
            </div>
            <div class="space-y-3">
              <div
                v-for="m in downloadedModels"
                :key="m.id"
                class="p-3 rounded-lg bg-white/5 border border-white/10 space-y-2"
              >
                <div class="text-white text-sm font-medium">{{ m.name }}</div>
                <div class="flex flex-wrap gap-2">
                  <label
                    v-for="lang in settings.favoriteLanguages"
                    :key="`${m.id}-${lang}`"
                    class="flex items-center gap-1.5 px-2 py-1 rounded bg-white/5 border border-white/10 cursor-pointer hover:bg-white/10 transition-colors"
                  >
                    <input
                      type="checkbox"
                      :checked="modelLanguagesFor(m.id).includes(lang)"
                      @change="handleModelLanguageToggle(m.id, lang)"
                      class="w-3.5 h-3.5 rounded border-white/30 bg-white/10 text-mic-listening focus:ring-mic-listening focus:ring-offset-0"
                    />
                    <span class="text-white text-xs">{{ LANGUAGE_DISPLAY_NAMES[lang] }}</span>
                  </label>
                </div>
              </div>
            </div>
          </div>

        </div>

        <!-- History Tab -->
        <div v-if="activeTab === 'history'" class="max-w-2xl space-y-6">
          <div class="flex items-center justify-between">
            <div>
              <h2 class="text-white text-xl font-semibold mb-1">Transcription History</h2>
              <p class="text-white/50 text-sm">Last 20 dictated texts</p>
            </div>
            <button
              v-if="history.length > 0"
              @click="handleClearHistory"
              class="px-4 py-2 rounded-lg bg-red-500/20 hover:bg-red-500/30 text-red-400 text-sm font-medium transition-colors flex items-center gap-2"
            >
              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
              </svg>
              Clear all
            </button>
          </div>

          <!-- Empty state -->
          <div v-if="history.length === 0" class="text-center py-16">
            <div class="w-16 h-16 mx-auto mb-4 rounded-full bg-white/5 flex items-center justify-center">
              <svg class="w-8 h-8 text-white/30" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
              </svg>
            </div>
            <p class="text-white/50">No transcriptions yet</p>
            <p class="text-white/30 text-sm mt-1">Dictated texts will appear here</p>
          </div>

          <!-- History list -->
          <div v-else class="space-y-3">
            <div
              v-for="entry in history"
              :key="entry.id"
              class="group p-4 rounded-xl bg-white/5 border border-white/10 hover:bg-white/10 transition-all"
            >
              <div class="flex items-start justify-between gap-4">
                <div class="flex-1 min-w-0">
                  <p class="text-white text-sm leading-relaxed break-words">{{ entry.text }}</p>
                  <div class="flex items-center gap-2 mt-2 flex-wrap">
                    <span class="text-white/40 text-xs">{{ formatDate(entry.timestamp) }}</span>
                    <span v-if="entry.modelId" class="text-xs px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-400">
                      {{ getShortModelName(entry.modelId) }}
                    </span>
                    <span v-if="entry.durationMs" class="text-xs px-1.5 py-0.5 rounded bg-green-500/20 text-green-400">
                      {{ formatDuration(entry.durationMs) }}
                    </span>
                  </div>
                </div>
                <button
                  @click="copyToClipboard(entry.text, entry.id)"
                  :class="[
                    'flex-shrink-0 p-2 rounded-lg transition-all',
                    copiedId === entry.id
                      ? 'bg-green-500/20 text-green-400'
                      : 'bg-white/10 hover:bg-white/20 text-white/60 hover:text-white opacity-0 group-hover:opacity-100'
                  ]"
                  :title="copiedId === entry.id ? 'Copied!' : 'Copy'"
                >
                  <svg v-if="copiedId === entry.id" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                  </svg>
                  <svg v-else class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"/>
                  </svg>
                </button>
              </div>
            </div>
          </div>

          <div v-if="history.length > 0" class="p-4 bg-white/5 rounded-xl border border-white/10">
            <p class="text-white/50 text-sm">
              <strong class="text-white/70">{{ history.length }}</strong> transcription{{ history.length > 1 ? 's' : '' }} in history
            </p>
          </div>
        </div>

        <!-- Models Tab -->
        <div v-if="activeTab === 'models'" class="max-w-2xl space-y-6">
          <div>
            <h2 class="text-white text-xl font-semibold mb-1">Whisper Models</h2>
            <p class="text-white/50 text-sm">Select the speech recognition model to use</p>
          </div>

          <div class="grid gap-4">
            <div
              v-for="model in models"
              :key="model.id"
              :class="[
                'p-5 rounded-xl border transition-all',
                settings.model === model.id
                  ? 'bg-mic-listening/10 border-mic-listening'
                  : 'bg-white/5 border-white/10 hover:bg-white/10'
              ]"
            >
              <div class="flex items-start justify-between">
                <div class="flex-1">
                  <div class="flex items-center gap-3">
                    <span class="text-white text-lg font-semibold">{{ model.name }}</span>
                    <span v-if="settings.model === model.id" class="text-xs bg-mic-listening/30 text-mic-listening px-2 py-1 rounded-full">
                      Active
                    </span>
                  </div>
                  <div class="flex items-center gap-3 mt-2">
                    <span class="text-white/50">{{ model.size }}</span>
                  </div>
                </div>

                <div class="flex items-center gap-3">
                  <!-- Select button -->
                  <button
                    v-if="settings.model !== model.id"
                    @click="handleSelectModel(model.id)"
                    :disabled="loadingModelId !== null"
                    class="px-4 py-2 rounded-lg bg-white/10 hover:bg-white/20 text-white text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
                  >
                    <template v-if="loadingModelId === model.id">
                      <svg class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
                        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"/>
                      </svg>
                      Loading...
                    </template>
                    <template v-else>
                      Use
                    </template>
                  </button>
                </div>
              </div>
            </div>
          </div>

          <div class="p-4 bg-blue-500/10 rounded-xl border border-blue-500/20">
            <p class="text-blue-400 text-sm">
              <strong>Tip:</strong> The "Small" model offers a good balance between speed and accuracy.
              For better accuracy, try "Large V3 Turbo".
            </p>
          </div>
        </div>

        <!-- Permissions Tab -->
        <div v-if="activeTab === 'permissions'" class="max-w-xl space-y-6">
          <div>
            <h2 class="text-white text-xl font-semibold mb-1">System Permissions</h2>
            <p class="text-white/50 text-sm">S2Tui requires certain permissions to work properly</p>
          </div>

          <!-- Microphone -->
          <div class="p-5 rounded-xl bg-white/5 border border-white/10">
            <div class="flex items-start gap-4">
              <div :class="[
                'w-12 h-12 rounded-xl flex items-center justify-center flex-shrink-0',
                permissions.microphone ? 'bg-green-500/20' : 'bg-amber-500/20'
              ]">
                <svg class="w-6 h-6" :class="permissions.microphone ? 'text-green-400' : 'text-amber-400'" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"/>
                </svg>
              </div>
              <div class="flex-1">
                <div class="flex items-center justify-between">
                  <p class="text-white font-semibold text-lg">Microphone</p>
                  <span v-if="permissions.microphone" class="text-green-400 flex items-center gap-1">
                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                    </svg>
                    Granted
                  </span>
                  <span v-else class="text-amber-400">Not granted</span>
                </div>
                <p class="text-white/50 text-sm mt-1">
                  Allows S2Tui to listen and transcribe your voice.
                </p>
              </div>
            </div>
          </div>

          <div class="p-4 bg-white/5 rounded-xl border border-white/10">
            <p class="text-white/50 text-sm">
              <strong class="text-white/70">Note:</strong> To modify microphone access, open
              <strong>System Settings → Privacy & Security → Microphone</strong>.
            </p>
          </div>
        </div>

        <!-- System Tab -->
        <div v-if="activeTab === 'system'" class="max-w-xl space-y-6">
          <div class="flex items-center justify-between">
            <div>
              <h2 class="text-white text-xl font-semibold mb-1">System Information</h2>
              <p class="text-white/50 text-sm">GPU acceleration and performance settings</p>
            </div>
            <button
              @click="refreshSystemHealth"
              class="p-2 rounded-lg bg-white/10 hover:bg-white/20 text-white/60 hover:text-white transition-colors"
              title="Refresh"
            >
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
          </div>

          <!-- GPU Status Card -->
          <div class="p-5 rounded-xl bg-white/5 border border-white/10">
            <div class="flex items-start gap-4">
              <div :class="[
                'w-12 h-12 rounded-xl flex items-center justify-center flex-shrink-0',
                (gpuStatus?.usingGpu || systemHealth?.vulkanAvailable || (systemHealth?.gpuBackend && systemHealth.gpuBackend !== 'cpu')) ? 'bg-green-500/20' : 'bg-amber-500/20'
              ]">
                <svg class="w-6 h-6" :class="(gpuStatus?.usingGpu || systemHealth?.vulkanAvailable || (systemHealth?.gpuBackend && systemHealth.gpuBackend !== 'cpu')) ? 'text-green-400' : 'text-amber-400'" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z" />
                </svg>
              </div>
              <div class="flex-1">
                <div class="flex items-center justify-between">
                  <p class="text-white font-semibold text-lg">GPU Acceleration</p>
                  <span :class="[
                    'px-2.5 py-1 rounded-full text-xs font-medium',
                    getBackendColor(gpuStatus?.backend || systemHealth?.gpuBackend)
                  ]">
                    {{ gpuStatus?.backend || systemHealth?.gpuBackend || 'Unknown' }}
                  </span>
                </div>
                <p class="text-white/50 text-sm mt-1">
                  <template v-if="systemHealth?.vulkanAvailable">
                    Vulkan is available{{ systemHealth?.vulkanVersion ? ` (${systemHealth.vulkanVersion})` : '' }}. GPU acceleration is active.
                  </template>
                  <template v-else-if="systemHealth?.gpuBackend === 'metal'">
                    Metal GPU is active. Your system is optimized for macOS.
                  </template>
                  <template v-else-if="systemHealth?.gpuBackend === 'cuda'">
                    CUDA GPU is active. Your system is optimized for NVIDIA.
                  </template>
                  <template v-else>
                    No GPU acceleration available. Running in CPU mode.
                  </template>
                </p>
                <div v-if="gpuStatus" class="mt-3 flex items-center gap-4 text-sm">
                  <span class="text-white/40">
                    Status: <span :class="gpuStatus.usingGpu ? 'text-green-400' : 'text-amber-400'">
                      {{ gpuStatus.usingGpu ? 'GPU Active' : 'CPU Mode' }}
                    </span>
                  </span>
                  <span v-if="gpuStatus.fallbackUsed" class="text-amber-400/80 text-xs">
                    (fallback from GPU failure)
                  </span>
                </div>
              </div>
            </div>
          </div>

          <!-- GPU Install Guide (shown only if no GPU backend available) -->
          <div v-if="systemHealth?.gpuBackend === 'cpu' && systemHealth?.installGuide" class="space-y-4">
            <div class="p-5 rounded-xl bg-amber-500/10 border border-amber-500/20">
              <div class="flex items-start gap-3">
                <svg class="w-5 h-5 text-amber-400 flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
                <div class="flex-1">
                  <p class="text-amber-400 font-medium">{{ systemHealth.installGuide.title }}</p>
                  <p class="text-amber-400/80 text-sm mt-1">{{ systemHealth.installGuide.description }}</p>
                </div>
              </div>
            </div>

            <!-- Installation Steps -->
            <div class="p-5 rounded-xl bg-white/5 border border-white/10 space-y-4">
              <p class="text-white font-medium">Installation Steps</p>
              <ol class="space-y-3">
                <li v-for="(step, index) in systemHealth.installGuide.steps" :key="index" class="flex items-start gap-3">
                  <span class="w-6 h-6 rounded-full bg-mic-listening/20 text-mic-listening text-xs flex items-center justify-center flex-shrink-0">
                    {{ index + 1 }}
                  </span>
                  <span class="text-white/70 text-sm">{{ step }}</span>
                </li>
              </ol>

              <!-- Terminal commands (Linux) -->
              <div v-if="systemHealth.installGuide.terminalCommands && systemHealth.installGuide.terminalCommands.length > 0" class="mt-4">
                <p class="text-white/60 text-sm mb-2">Run in terminal:</p>
                <div class="bg-black/40 rounded-lg p-3 font-mono text-sm">
                  <code v-for="(cmd, index) in systemHealth.installGuide.terminalCommands" :key="index" class="block text-green-400">
                    $ {{ cmd }}
                  </code>
                </div>
              </div>

              <!-- Download links (Windows) -->
              <div v-if="systemHealth.installGuide.downloadUrls && systemHealth.installGuide.downloadUrls.length > 0" class="mt-4 space-y-2">
                <p class="text-white/60 text-sm">Download drivers:</p>
                <a
                  v-for="link in systemHealth.installGuide.downloadUrls"
                  :key="link.url"
                  :href="link.url"
                  target="_blank"
                  class="flex items-center justify-between p-3 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 transition-colors group"
                >
                  <div>
                    <p class="text-white text-sm font-medium group-hover:text-mic-listening transition-colors">{{ link.name }}</p>
                    <p class="text-white/40 text-xs">{{ link.description }}</p>
                  </div>
                  <svg class="w-5 h-5 text-white/40 group-hover:text-mic-listening transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                  </svg>
                </a>
              </div>
            </div>
          </div>

          <!-- System Info Footer -->
          <div class="p-4 bg-white/5 rounded-xl border border-white/10 space-y-2">
            <p class="text-white/70 text-sm font-medium">System Details</p>
            <div class="grid grid-cols-2 gap-2 text-sm">
              <span class="text-white/40">Platform:</span>
              <span class="text-white/70">{{ systemHealth?.osInfo?.platform || 'Unknown' }}</span>
              <span class="text-white/40">OS Version:</span>
              <span class="text-white/70">{{ systemHealth?.osInfo?.version || 'Unknown' }}</span>
              <template v-if="systemHealth?.osInfo?.distribution">
                <span class="text-white/40">Distribution:</span>
                <span class="text-white/70">{{ systemHealth.osInfo.distribution }}</span>
              </template>
              <span class="text-white/40">GPU Backend:</span>
              <span class="text-white/70">{{ systemHealth?.gpuBackend || 'None' }}</span>
            </div>
          </div>

          <div class="p-4 bg-blue-500/10 rounded-xl border border-blue-500/20">
            <p class="text-blue-400 text-sm">
              <strong>Tip:</strong> GPU acceleration (Vulkan/Metal/CUDA) significantly speeds up transcription.
            </p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Custom scrollbar */
::-webkit-scrollbar {
  width: 8px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
  border-radius: 4px;
}
::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3);
}
</style>
