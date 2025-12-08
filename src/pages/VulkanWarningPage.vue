<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { SystemHealth } from "../stores/appStore";

const systemHealth = ref<SystemHealth | null>(null);
const isLoading = ref(true);
const copiedCommand = ref<string | null>(null);

onMounted(async () => {
  try {
    systemHealth.value = await invoke<SystemHealth>("check_system_health");
  } catch {
    // System health check failed, continue with defaults
  } finally {
    isLoading.value = false;
  }
});

const installGuide = computed(() => systemHealth.value?.installGuide);
const osInfo = computed(() => systemHealth.value?.osInfo);
const isWindows = computed(() => osInfo.value?.platform === "windows");
const isLinux = computed(() => osInfo.value?.platform === "linux");

async function retryAfterInstall() {
  // Restart the application to re-check Vulkan availability
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

async function quitApp() {
  const { exit } = await import("@tauri-apps/plugin-process");
  await exit(0);
}

async function openUrl(url: string) {
  const { open } = await import("@tauri-apps/plugin-shell");
  await open(url);
}

async function copyCommand(command: string) {
  const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
  await writeText(command);
  copiedCommand.value = command;
  setTimeout(() => {
    copiedCommand.value = null;
  }, 2000);
}
</script>

<template>
  <div class="h-full bg-gray-900 flex flex-col overflow-hidden">
    <!-- Loading State -->
    <div v-if="isLoading" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <svg class="w-10 h-10 text-amber-400 animate-spin mx-auto mb-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
        </svg>
        <p class="text-white/70">Checking system...</p>
      </div>
    </div>

    <!-- Main Content -->
    <div v-else class="flex-1 overflow-y-auto p-5">
      <!-- Header -->
      <div class="flex items-center gap-3 mb-4">
        <div class="w-12 h-12 rounded-xl bg-red-500/20 flex items-center justify-center flex-shrink-0">
          <svg class="w-6 h-6 text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
        </div>
        <div>
          <h1 class="text-white font-bold text-lg">Vulkan Required</h1>
          <p class="text-red-400 text-sm">GPU drivers needed for transcription</p>
        </div>
      </div>

      <!-- Info Box -->
      <div class="bg-red-500/10 rounded-xl p-4 mb-5 border border-red-500/20">
        <div class="flex items-start gap-3">
          <svg class="w-5 h-5 text-red-400 mt-0.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <div>
            <p class="text-red-300 text-sm font-medium">Vulkan is required to run S2Tui</p>
            <p class="text-red-200/70 text-xs mt-1">
              Please install or update your GPU drivers to enable Vulkan support.
              S2Tui uses GPU acceleration for fast speech-to-text transcription.
            </p>
          </div>
        </div>
      </div>

      <!-- Fallback when Vulkan is actually available (debug mode) -->
      <div v-if="!installGuide && systemHealth?.vulkanAvailable" class="bg-green-500/10 rounded-xl p-4 border border-green-500/20">
        <div class="flex items-center gap-3">
          <svg class="w-6 h-6 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
          <div>
            <p class="text-green-300 font-medium">Vulkan is already available!</p>
            <p class="text-green-200/70 text-sm mt-1">
              GPU acceleration is enabled. This modal is shown for debugging purposes.
            </p>
          </div>
        </div>
      </div>

      <!-- Installation Guide Content -->
      <div v-if="installGuide" class="space-y-4">
        <!-- Guide Title & Description -->
        <div>
          <h2 class="text-white font-semibold">{{ installGuide.title }}</h2>
          <p class="text-white/60 text-sm mt-1">{{ installGuide.description }}</p>
        </div>

        <!-- Windows: Download Links -->
        <div v-if="isWindows && installGuide.downloadUrls && installGuide.downloadUrls.length > 0">
          <p class="text-white/50 text-xs uppercase tracking-wider font-medium mb-2">Download Drivers</p>
          <div class="space-y-2">
            <a
              v-for="link in installGuide.downloadUrls"
              :key="link.url"
              @click.prevent="openUrl(link.url)"
              href="#"
              class="flex items-center justify-between p-3 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 hover:border-white/20 transition-all cursor-pointer group"
            >
              <div>
                <p class="text-white text-sm font-medium group-hover:text-amber-400 transition-colors">{{ link.name }}</p>
                <p class="text-white/50 text-xs">{{ link.description }}</p>
              </div>
              <svg class="w-5 h-5 text-white/40 group-hover:text-amber-400 transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </a>
          </div>
        </div>

        <!-- Linux: Terminal Commands -->
        <div v-if="isLinux && installGuide.terminalCommands && installGuide.terminalCommands.length > 0">
          <p class="text-white/50 text-xs uppercase tracking-wider font-medium mb-2">Terminal Commands</p>
          <div class="space-y-2">
            <div
              v-for="(cmd, index) in installGuide.terminalCommands"
              :key="index"
              class="group relative"
            >
              <code
                class="block p-3 pr-12 rounded-lg bg-black/40 text-sm font-mono break-all"
                :class="cmd.startsWith('#') ? 'text-white/40 italic' : 'text-green-400'"
              >{{ cmd }}</code>
              <button
                v-if="!cmd.startsWith('#')"
                @click="copyCommand(cmd)"
                class="absolute right-2 top-1/2 -translate-y-1/2 p-1.5 rounded bg-white/10 hover:bg-white/20 transition-all"
                :class="copiedCommand === cmd ? 'bg-green-500/20' : ''"
                title="Copy to clipboard"
              >
                <svg v-if="copiedCommand !== cmd" class="w-4 h-4 text-white/70" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                </svg>
                <svg v-else class="w-4 h-4 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                </svg>
              </button>
            </div>
          </div>
        </div>

        <!-- Installation Steps -->
        <div v-if="installGuide.steps && installGuide.steps.length > 0" class="bg-white/5 rounded-xl p-4">
          <p class="text-white/50 text-xs uppercase tracking-wider font-medium mb-3">Steps</p>
          <ol class="space-y-2">
            <li
              v-for="(step, index) in installGuide.steps"
              :key="index"
              class="flex items-start gap-3"
            >
              <span class="w-6 h-6 rounded-full bg-amber-500/20 text-amber-400 text-xs flex items-center justify-center flex-shrink-0 font-medium">
                {{ index + 1 }}
              </span>
              <span class="text-white/80 text-sm">{{ step }}</span>
            </li>
          </ol>
        </div>

        <!-- System Info -->
        <div v-if="osInfo" class="text-center pt-2">
          <p class="text-white/30 text-xs">
            Detected: {{ osInfo.platform }}{{ osInfo.distribution ? ` (${osInfo.distribution})` : '' }}{{ osInfo.version ? ` ${osInfo.version}` : '' }}
          </p>
        </div>
      </div>
    </div>

    <!-- Footer Actions (fixed) -->
    <div class="flex-shrink-0 p-4 border-t border-white/10 bg-gray-900 space-y-2">
      <button
        @click="retryAfterInstall"
        class="w-full px-4 py-2.5 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition-colors flex items-center justify-center gap-2"
      >
        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        I've installed Vulkan - Retry
      </button>
      <button
        @click="quitApp"
        class="w-full px-4 py-2 text-white/50 hover:text-white/70 hover:bg-white/5 rounded-lg transition-colors text-sm"
      >
        Quit Application
      </button>
    </div>
  </div>
</template>

<style>
/* Custom scrollbar */
::-webkit-scrollbar {
  width: 6px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.15);
  border-radius: 3px;
}
::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.25);
}
</style>
