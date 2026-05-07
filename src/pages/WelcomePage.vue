<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { SystemHealth } from "../stores/appStore";

const systemHealth = ref<SystemHealth | null>(null);
const isLoading = ref(true);
const dontShowAgain = ref(false);
const emailCopied = ref(false);

// First-launch model-download state. Populated from
// `list_required_models` on mount; updated by Tauri events emitted by
// the main window's download orchestrator. The "Get started" CTA stays
// disabled until every entry reaches `done` so the user can't dismiss
// the welcome window while their app is still unusable.
interface DownloadItem {
  id: string;
  displayName: string;
  sizeBytes: number;
  bytesReceived: number;
  percent: number;
  status: "pending" | "downloading" | "done" | "error";
  errorMessage?: string;
}
interface RequiredModelInfo {
  id: string;
  displayName: string;
  filename: string;
  sizeBytes: number;
  url: string;
  present: boolean;
}
const downloadItems = ref<DownloadItem[]>([]);
const allDownloadsDone = computed(
  () =>
    downloadItems.value.length === 0 ||
    downloadItems.value.every((i) => i.status === "done"),
);
const anyDownloadFailed = computed(() =>
  downloadItems.value.some((i) => i.status === "error"),
);

let unlistenProgress: UnlistenFn | null = null;
let unlistenComplete: UnlistenFn | null = null;
let unlistenError: UnlistenFn | null = null;

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

onMounted(async () => {
  try {
    systemHealth.value = await invoke<SystemHealth>("check_system_health");
  } catch {
    // System health check failed, continue with defaults
  } finally {
    isLoading.value = false;
  }

  // Pull the list of required models. The ones currently missing will
  // be downloaded by the main window — we just render their progress.
  try {
    const required = await invoke<RequiredModelInfo[]>("list_required_models");
    downloadItems.value = required
      .filter((m) => !m.present)
      .map((m) => ({
        id: m.id,
        displayName: m.displayName,
        sizeBytes: m.sizeBytes,
        bytesReceived: 0,
        percent: 0,
        status: "pending" as const,
      }));
  } catch {
    // If we can't even ask the backend, leave the section empty —
    // the user just sees the welcome content as before.
  }

  unlistenProgress = await listen<{
    model: string;
    bytesReceived: number;
    totalBytes: number;
    percent: number;
  }>("model:download:progress", (e) => {
    const it = downloadItems.value.find((i) => i.id === e.payload.model);
    if (it) {
      it.status = "downloading";
      it.bytesReceived = e.payload.bytesReceived;
      it.percent = e.payload.percent;
    }
  });
  unlistenComplete = await listen<{ model: string }>(
    "model:download:complete",
    (e) => {
      const it = downloadItems.value.find((i) => i.id === e.payload.model);
      if (it) {
        it.status = "done";
        it.percent = 100;
      }
    },
  );
  unlistenError = await listen<{ model: string; message: string }>(
    "model:download:error",
    (e) => {
      const it = downloadItems.value.find((i) => i.id === e.payload.model);
      if (it) {
        it.status = "error";
        it.errorMessage = e.payload.message;
      }
    },
  );
});

onUnmounted(() => {
  unlistenProgress?.();
  unlistenComplete?.();
  unlistenError?.();
});

const gpuEnabled = computed(() => {
  if (!systemHealth.value) return false;
  const platform = systemHealth.value.osInfo.platform;
  if (platform === "macos") return true; // Metal is always available on macOS
  return systemHealth.value.vulkanAvailable;
});

const gpuBackend = computed(() => {
  if (!systemHealth.value) return "Unknown";
  return systemHealth.value.gpuBackend;
});

function closeWindow() {
  if (dontShowAgain.value) {
    emit("welcome:dismissed", { permanent: true });
  }
  getCurrentWebviewWindow().close();
}

function openUrl(url: string) {
  import("@tauri-apps/plugin-shell").then(({ open }) => {
    open(url);
  });
}

async function copyEmail() {
  const email = "clement.baranger@accessdevops.com";
  try {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    await writeText(email);
    emailCopied.value = true;
    setTimeout(() => {
      emailCopied.value = false;
    }, 2000);
  } catch {
    // Clipboard operation failed
  }
}

function handleDragStart(event: MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement;
  // Don't start drag on interactive elements
  if (
    target.tagName === "BUTTON" ||
    target.closest("button") ||
    target.tagName === "A" ||
    target.closest("a") ||
    target.tagName === "INPUT" ||
    target.closest("input") ||
    target.tagName === "LABEL" ||
    target.closest("label")
  ) {
    return;
  }

  // Use threshold to differentiate click/select from drag
  const startX = event.clientX;
  const startY = event.clientY;
  let hasMoved = false;

  const onMouseMove = (e: MouseEvent) => {
    const dx = Math.abs(e.clientX - startX);
    const dy = Math.abs(e.clientY - startY);

    // Only start dragging if movement exceeds 5px threshold
    if (!hasMoved && (dx > 5 || dy > 5)) {
      hasMoved = true;
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
      getCurrentWebviewWindow().startDragging();
    }
  };

  const onMouseUp = () => {
    window.removeEventListener("mousemove", onMouseMove);
    window.removeEventListener("mouseup", onMouseUp);
  };

  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp);
}
</script>

<template>
  <div
    class="h-screen bg-gradient-to-br from-gray-900 via-gray-800 to-gray-900 flex flex-col"
    @mousedown="handleDragStart"
  >
    <!-- Fixed Header with Close Button -->
    <div class="flex-shrink-0 flex justify-end p-4">
      <button
        type="button"
        @click="closeWindow"
        class="w-8 h-8 rounded-full bg-white/10 hover:bg-white/20 flex items-center justify-center text-white/60 hover:text-white transition-all z-10"
        title="Close"
      >
        <svg class="w-4 h-4 pointer-events-none" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- Scrollable Content -->
    <div class="flex-1 overflow-y-auto px-6 pb-6">
      <div class="w-full max-w-lg mx-auto">
        <!-- Loading State -->
        <div v-if="isLoading" class="text-center py-20">
          <div class="w-20 h-20 mx-auto mb-6 rounded-full bg-blue-500/20 flex items-center justify-center">
            <svg class="w-10 h-10 text-blue-400 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
          </div>
          <h2 class="text-2xl font-bold text-white mb-2">Loading...</h2>
        </div>

        <!-- Main Content -->
        <div v-else class="glass rounded-2xl p-6 shadow-2xl border border-white/10">
          <!-- Header with Logo -->
          <div class="text-center mb-5">
            <div class="w-16 h-16 mx-auto mb-3 rounded-2xl bg-gradient-to-br from-blue-500 to-purple-600 p-0.5 shadow-lg shadow-blue-500/25">
              <img
                src="/src-tauri/icons/128x128.png"
                alt="S2Tui Logo"
                class="w-full h-full rounded-2xl"
              />
            </div>
            <h1 class="text-xl font-bold text-white mb-2">Welcome to S2Tui</h1>
            <div class="flex items-center justify-center gap-2 flex-wrap">
              <span class="px-3 py-1.5 rounded-full text-xs font-semibold bg-green-500/20 text-green-400 border border-green-500/30">
                100% Free
              </span>
              <span class="px-3 py-1.5 rounded-full text-xs font-semibold bg-blue-500/20 text-blue-400 border border-blue-500/30">
                100% Local & Private
              </span>
              <span class="px-3 py-1.5 rounded-full text-xs font-semibold bg-purple-500/20 text-purple-400 border border-purple-500/30">
                100% Open Source
              </span>
            </div>
          </div>

          <!-- Support Section (moved from bottom) -->
          <div class="text-center mb-5">
            <p class="text-white/70 text-sm mb-2">
              S2Tui is free forever, even for commercial use.
            </p>
            <p class="text-white/60 text-xs">
              Feel free to contact me:
              <span class="relative inline-block">
                <button
                  type="button"
                  @click="copyEmail"
                  class="text-blue-400 hover:text-blue-300 transition-colors cursor-pointer"
                  title="Click to copy email"
                >
                  clement.baranger@accessdevops.com
                </button>
                <span
                  v-if="emailCopied"
                  class="absolute -top-8 left-1/2 -translate-x-1/2 px-2 py-1 bg-green-500 text-white text-xs rounded shadow-lg whitespace-nowrap animate-fade-in"
                >
                  Copied!
                </span>
              </span>
            </p>
          </div>

          <!-- First-launch model download (only renders when at least
               one Whisper model is missing on disk). Models are no longer
               bundled with the app — they're fetched from the
               `models-v1` GitHub Release on demand. The Get Started
               button stays disabled while the downloads are running. -->
          <div
            v-if="downloadItems.length > 0"
            class="mb-3 p-3 rounded-xl bg-white/5 border border-white/10 space-y-2"
          >
            <div>
              <h3 class="text-white text-sm font-semibold">Setting up speech recognition</h3>
              <p class="text-white/50 text-xs">First-time download — needed once. Total ~728 MB.</p>
            </div>
            <div
              v-for="item in downloadItems"
              :key="item.id"
              class="space-y-1"
            >
              <div class="flex items-center justify-between text-xs">
                <span class="text-white">{{ item.displayName }}</span>
                <span class="text-white/40">
                  <span v-if="item.status === 'pending'">Pending</span>
                  <span v-else-if="item.status === 'downloading'">
                    {{ formatBytes(item.bytesReceived) }} / {{ formatBytes(item.sizeBytes) }}
                    ({{ item.percent }}%)
                  </span>
                  <span v-else-if="item.status === 'done'" class="text-green-400">Done</span>
                  <span v-else-if="item.status === 'error'" class="text-red-400">Failed</span>
                </span>
              </div>
              <div class="h-1.5 rounded-full bg-white/10 overflow-hidden">
                <div
                  class="h-full transition-all"
                  :class="item.status === 'error' ? 'bg-red-500' : 'bg-blue-500'"
                  :style="{ width: `${item.percent}%` }"
                ></div>
              </div>
              <p v-if="item.errorMessage" class="text-red-400 text-xs">
                {{ item.errorMessage }}
              </p>
            </div>
            <p
              v-if="anyDownloadFailed"
              class="text-amber-300/80 text-xs pt-1"
            >
              A download failed. Restart the app to retry.
            </p>
          </div>

          <!-- GPU Status Section -->
          <div class="mb-5">
            <div :class="[
              'rounded-xl p-3 border',
              gpuEnabled ? 'bg-green-500/10 border-green-500/20' : 'bg-amber-500/10 border-amber-500/20'
            ]">
              <div class="flex items-start gap-3">
                <div :class="[
                  'w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0',
                  gpuEnabled ? 'bg-green-500/20' : 'bg-amber-500/20'
                ]">
                  <svg :class="gpuEnabled ? 'w-4 h-4 text-green-400' : 'w-4 h-4 text-amber-400'" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                </div>
                <div class="flex-1">
                  <div class="flex items-center justify-between">
                    <p :class="gpuEnabled ? 'text-green-200 font-medium text-sm' : 'text-amber-200 font-medium text-sm'">
                      GPU Acceleration
                    </p>
                    <span :class="[
                      'px-2 py-0.5 rounded text-xs font-medium',
                      gpuEnabled ? 'bg-green-500/30 text-green-300' : 'bg-amber-500/30 text-amber-300'
                    ]">
                      {{ gpuBackend }}
                    </span>
                  </div>
                  <p :class="gpuEnabled ? 'text-green-200/70 text-xs mt-1' : 'text-amber-200/70 text-xs mt-1'">
                    <template v-if="gpuEnabled">
                      GPU acceleration is enabled. Transcription takes ~4 seconds on average.
                    </template>
                    <template v-else>
                      GPU acceleration unavailable. Transcription may take up to 90 seconds.
                    </template>
                  </p>
                </div>
              </div>
            </div>
            <p class="text-white/40 text-xs mt-1.5 text-center">
              S2Tui uses Metal (macOS) or Vulkan (Windows/Linux) for hardware acceleration.
            </p>
          </div>

          <!-- Links Section -->
          <div class="space-y-2 mb-5">
            <button
              type="button"
              @click="openUrl('https://s2tui.accessdevops.com')"
              class="w-full flex items-center justify-between p-2.5 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 hover:border-white/20 transition-all"
            >
              <div class="flex items-center gap-3">
                <svg class="w-4 h-4 text-blue-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
                </svg>
                <div class="text-left">
                  <span class="text-white text-xs block">Check for updates:</span>
                  <span class="text-blue-400 text-xs">s2tui.accessdevops.com</span>
                </div>
              </div>
              <svg class="w-4 h-4 text-white/40" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </button>

            <button
              type="button"
              @click="openUrl('https://github.com/AccessDevops/S2Tui/issues')"
              class="w-full flex items-center justify-between p-2.5 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 hover:border-white/20 transition-all"
            >
              <div class="flex items-center gap-3">
                <svg class="w-4 h-4 text-purple-400" fill="currentColor" viewBox="0 0 24 24">
                  <path fill-rule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" clip-rule="evenodd" />
                </svg>
                <span class="text-white text-xs">Report issues or suggest features</span>
              </div>
              <svg class="w-4 h-4 text-white/40" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </button>
          </div>

          <!-- Actions -->
          <div class="flex flex-col gap-2">
            <button
              type="button"
              :disabled="!allDownloadsDone"
              @click="allDownloadsDone && closeWindow()"
              :class="[
                'w-full px-4 py-2.5 rounded-xl font-medium transition-all shadow-lg text-sm',
                allDownloadsDone
                  ? 'bg-gradient-to-r from-blue-500 to-purple-600 hover:from-blue-600 hover:to-purple-700 text-white shadow-blue-500/25'
                  : 'bg-white/10 text-white/40 cursor-not-allowed',
              ]"
            >
              <span v-if="downloadItems.length === 0 || allDownloadsDone">Get Started</span>
              <span v-else>Downloading models…</span>
            </button>
            <label class="flex items-center justify-center gap-2 cursor-pointer py-1">
              <input
                type="checkbox"
                v-model="dontShowAgain"
                class="w-4 h-4 rounded border-white/30 bg-white/10 text-blue-500 focus:ring-blue-500 focus:ring-offset-0 cursor-pointer"
              />
              <span class="text-white/50 text-xs select-none">Don't show this again</span>
            </label>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style>
.glass {
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(20px);
}

@keyframes fade-in {
  from {
    opacity: 0;
    transform: translateX(-50%) translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }
}

.animate-fade-in {
  animation: fade-in 0.2s ease-out;
}
</style>
