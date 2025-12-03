<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from "vue";
import { useAppStore, type Language, type ModelId } from "../stores/appStore";
import { useTauri } from "../composables/useTauri";
import { saveSettings, loadHistory, clearHistory as clearHistoryStore } from "../composables/useStore";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import ShortcutCapture from "../components/ShortcutCapture.vue";

const store = useAppStore();
const { setLanguage, setShortcut, loadWhisperModel, initListeners } = useTauri();

const shortcutError = ref<string | null>(null);

const settings = computed(() => store.settings);
const models = computed(() => store.models);
const permissions = computed(() => store.permissions);
const history = computed(() => store.history);

const activeTab = ref<"general" | "models" | "permissions" | "history">("general");
const copiedId = ref<string | null>(null);
const loadingModelId = ref<ModelId | null>(null);

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    closeWindow();
  }
}

onMounted(async () => {
  initListeners();
  // Load history from persistence
  const savedHistory = await loadHistory();
  store.setHistory(savedHistory);

  // Listen for Escape key to close window
  window.addEventListener("keydown", handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener("keydown", handleKeydown);
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
  store.updateSettings({ language: lang });
  await setLanguage(lang);
  await saveSettings({ language: lang });
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

// Model handlers
async function handleSelectModel(modelId: ModelId) {
  const model = models.value.find((m) => m.id === modelId);
  if (model && model.downloaded) {
    loadingModelId.value = modelId;
    try {
      // Use the best available quantization for this specific model
      const quant = store.getBestQuantForModel(modelId);
      await loadWhisperModel(modelId, quant);
    } catch (error) {
      console.error("Failed to load model:", error);
    } finally {
      loadingModelId.value = null;
    }
  }
}

async function closeWindow() {
  const window = getCurrentWebviewWindow();
  await window.close();
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
          <p class="text-white/30 text-xs">S2Tui v0.1.0</p>
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
              @change="handleShortcutChange"
            />
            <p v-if="shortcutError" class="text-red-400 text-sm flex items-center gap-2">
              <svg class="w-4 h-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {{ shortcutError }}
            </p>
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
              <strong class="text-white/70">Note:</strong> Permissions are managed by macOS.
              Open <strong>System Preferences → Security & Privacy → Privacy</strong> to modify them.
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
