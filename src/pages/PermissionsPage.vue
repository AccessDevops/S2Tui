<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const permissionType = ref<"microphone" | "accessibility">("microphone");
const isGranted = ref(false);
const isChecking = ref(false);

// Get permission type from URL params
onMounted(() => {
  const params = new URLSearchParams(window.location.search);
  const type = params.get("type");
  if (type === "accessibility") {
    permissionType.value = "accessibility";
  }
});

const title = ref("Microphone access required");
const description = ref("S2Tui needs access to your microphone for speech recognition.");

const steps = [
  "Open System Preferences > Security & Privacy",
  "Click on the Privacy tab",
  "Select Microphone from the list",
  "Check the box next to S2Tui",
];

async function openSystemPreferences() {
  // Open System Preferences to Privacy settings
  if (permissionType.value === "microphone") {
    window.open("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
  } else {
    window.open("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
  }
}

async function checkPermission() {
  isChecking.value = true;
  try {
    const perms = await invoke<{ microphone: boolean }>("check_permissions");
    if (permissionType.value === "microphone" && perms.microphone) {
      isGranted.value = true;
      // Close window after a short delay
      setTimeout(async () => {
        await getCurrentWebviewWindow().close();
      }, 1000);
    }
  } catch (e) {
    console.error("Failed to check permissions:", e);
  } finally {
    isChecking.value = false;
  }
}

async function closeWindow() {
  await getCurrentWebviewWindow().close();
}

async function startDrag(event: MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement;
  // Don't drag when clicking buttons
  if (target.tagName === "BUTTON" || target.closest("button")) return;
  await getCurrentWebviewWindow().startDragging();
}
</script>

<template>
  <div
    class="min-h-screen bg-gradient-to-br from-gray-900 via-gray-800 to-gray-900 flex items-center justify-center p-6"
    @mousedown="startDrag"
  >
    <div class="w-full max-w-md">
      <!-- Success State -->
      <div v-if="isGranted" class="text-center">
        <div class="w-20 h-20 mx-auto mb-6 rounded-full bg-green-500/20 flex items-center justify-center">
          <svg class="w-10 h-10 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
        </div>
        <h2 class="text-2xl font-bold text-white mb-2">Permission Granted!</h2>
        <p class="text-white/60">You can now use S2Tui for speech recognition.</p>
      </div>

      <!-- Permission Request -->
      <div v-else class="glass rounded-2xl p-8 shadow-2xl border border-white/10">
        <!-- Header -->
        <div class="flex items-center gap-4 mb-6">
          <div class="w-14 h-14 rounded-full bg-amber-500/20 flex items-center justify-center flex-shrink-0">
            <svg class="w-7 h-7 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
              />
            </svg>
          </div>
          <div>
            <h2 class="text-xl font-bold text-white">{{ title }}</h2>
            <p class="text-white/60 text-sm mt-1">{{ description }}</p>
          </div>
        </div>

        <!-- Steps -->
        <div class="bg-white/5 rounded-xl p-5 mb-6">
          <p class="text-white/50 text-xs uppercase tracking-wider font-medium mb-3">Steps to follow</p>
          <ol class="space-y-3">
            <li
              v-for="(step, index) in steps"
              :key="index"
              class="flex items-start gap-3 text-white/80"
            >
              <span class="w-6 h-6 rounded-full bg-white/10 flex items-center justify-center text-sm font-medium flex-shrink-0 mt-0.5">
                {{ index + 1 }}
              </span>
              <span class="text-sm leading-relaxed">{{ step }}</span>
            </li>
          </ol>
        </div>

        <!-- Actions -->
        <div class="flex gap-3">
          <button
            @click="openSystemPreferences"
            class="flex-1 px-5 py-3 bg-gradient-to-r from-blue-500 to-blue-600 hover:from-blue-600 hover:to-blue-700 text-white rounded-xl font-medium transition-all shadow-lg shadow-blue-500/25 text-sm"
          >
            Open System Preferences
          </button>
          <button
            @click="checkPermission"
            :disabled="isChecking"
            class="px-5 py-3 bg-white/10 hover:bg-white/20 text-white rounded-xl font-medium transition-all text-sm disabled:opacity-50"
          >
            <span v-if="isChecking">Checking...</span>
            <span v-else>Check</span>
          </button>
        </div>

        <button
          @click="closeWindow"
          class="w-full mt-4 px-4 py-2 text-white/50 hover:text-white/70 transition-colors text-sm"
        >
          I'll do it later
        </button>
      </div>
    </div>
  </div>
</template>

<style>
.glass {
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(20px);
}
</style>
