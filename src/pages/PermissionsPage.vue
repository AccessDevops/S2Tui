<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { platform } from "@tauri-apps/plugin-os";

const permissionType = ref<"microphone" | "accessibility">("microphone");
const isGranted = ref(false);
const isRequesting = ref(false);
const wasDenied = ref(false);
const currentPlatform = ref<string>("macos");

// Get permission type from URL params and auto-request permission
onMounted(async () => {
  // Detect platform
  try {
    currentPlatform.value = await platform();
  } catch (e) {
    console.error("Failed to detect platform:", e);
  }

  const params = new URLSearchParams(window.location.search);
  const type = params.get("type");
  if (type === "accessibility") {
    permissionType.value = "accessibility";
  }

  // Automatically request permission on mount
  // This will trigger the native permission dialog if not yet determined
  await requestPermission();
});

const title = computed(() => {
  if (currentPlatform.value === "windows") {
    return "Microphone access required";
  } else if (currentPlatform.value === "linux") {
    return "Microphone access";
  } else {
    return "Microphone access required";
  }
});

const description = computed(() => {
  if (currentPlatform.value === "windows") {
    return "S2Tui needs microphone permissions to be enabled in Windows Settings.";
  } else if (currentPlatform.value === "linux") {
    return "S2Tui needs access to your audio devices for speech recognition.";
  } else {
    return "S2Tui needs access to your microphone for speech recognition.";
  }
});

async function requestPermission() {
  isRequesting.value = true;
  try {
    // This will trigger the native macOS permission dialog
    // if the permission status is "NotDetermined"
    const granted = await invoke<boolean>("request_microphone_permission");

    if (granted) {
      isGranted.value = true;
      // Notify main window that permission was granted
      await emit("permission:granted", { type: permissionType.value });
      // Close window after a short delay
      setTimeout(async () => {
        await getCurrentWebviewWindow().close();
      }, 1500);
    } else {
      // Permission was denied - user needs to enable manually in System Preferences
      wasDenied.value = true;
    }
  } catch (e) {
    console.error("Failed to request permission:", e);
    wasDenied.value = true;
  } finally {
    isRequesting.value = false;
  }
}

async function openSystemPreferences() {
  // Open system settings specific to each platform
  if (currentPlatform.value === "macos") {
    if (permissionType.value === "microphone") {
      window.open("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
    } else {
      window.open("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
    }
  } else if (currentPlatform.value === "windows") {
    // Windows: The backend will open ms-settings:privacy-microphone via request_microphone_permission
    // So we just call it again
    await requestPermission();
  } else if (currentPlatform.value === "linux") {
    // Linux: Instructions are shown in the UI via platformInstructions
  }
}

// Computed property for platform-specific instructions
const platformInstructions = computed(() => {
  if (currentPlatform.value === "windows") {
    return [
      "Open Settings > Privacy & security > Microphone",
      "Enable 'Microphone access' and 'Let apps access your microphone'",
      "Ensure S2Tui is in the allowed apps list"
    ];
  } else if (currentPlatform.value === "linux") {
    return [
      "Ensure your user is in the 'audio' group: groups | grep audio",
      "Check audio devices exist: ls -l /dev/snd/",
      "Verify capture devices: arecord -l"
    ];
  } else {
    // macOS
    return [
      "Click 'Open System Settings' below",
      "Find and enable S2Tui in the list",
      "Click 'Check' to verify access"
    ];
  }
});

async function checkPermission() {
  isRequesting.value = true;
  try {
    const perms = await invoke<{ microphone: boolean }>("check_permissions");
    if (permissionType.value === "microphone" && perms.microphone) {
      isGranted.value = true;
      // Notify main window that permission was granted
      await emit("permission:granted", { type: permissionType.value });
      // Close window after a short delay
      setTimeout(async () => {
        await getCurrentWebviewWindow().close();
      }, 1000);
    }
  } catch (e) {
    console.error("Failed to check permissions:", e);
  } finally {
    isRequesting.value = false;
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
      <!-- Loading State -->
      <div v-if="isRequesting && !wasDenied" class="text-center">
        <div class="w-20 h-20 mx-auto mb-6 rounded-full bg-blue-500/20 flex items-center justify-center">
          <svg class="w-10 h-10 text-blue-400 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
        </div>
        <h2 class="text-2xl font-bold text-white mb-2">Requesting access...</h2>
        <p class="text-white/60">Please allow microphone access in the system dialog.</p>
      </div>

      <!-- Success State -->
      <div v-else-if="isGranted" class="text-center">
        <div class="w-20 h-20 mx-auto mb-6 rounded-full bg-green-500/20 flex items-center justify-center">
          <svg class="w-10 h-10 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
        </div>
        <h2 class="text-2xl font-bold text-white mb-2">Permission Granted!</h2>
        <p class="text-white/60 mb-6">You can now use S2Tui for speech recognition.</p>
        <button
          @click="closeWindow"
          class="px-8 py-3 bg-gradient-to-r from-green-500 to-green-600 hover:from-green-600 hover:to-green-700 text-white rounded-xl font-medium transition-all shadow-lg shadow-green-500/25"
        >
          Close
        </button>
      </div>

      <!-- Denied State - Manual instructions -->
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

        <!-- Instructions -->
        <div class="bg-white/5 rounded-xl p-5 mb-6">
          <p class="text-white/50 text-xs uppercase tracking-wider font-medium mb-3">
            {{ wasDenied ? 'Enable access manually' : 'Steps to follow' }}
          </p>
          <ol class="space-y-3">
            <li
              v-for="(instruction, index) in platformInstructions"
              :key="index"
              class="flex items-start gap-3 text-white/80"
            >
              <span class="w-6 h-6 rounded-full bg-white/10 flex items-center justify-center text-sm font-medium flex-shrink-0 mt-0.5">
                {{ index + 1 }}
              </span>
              <span class="text-sm leading-relaxed" v-html="instruction"></span>
            </li>
          </ol>
        </div>

        <!-- Actions -->
        <div class="flex gap-3">
          <button
            @click="openSystemPreferences"
            class="flex-1 px-5 py-3 bg-gradient-to-r from-blue-500 to-blue-600 hover:from-blue-600 hover:to-blue-700 text-white rounded-xl font-medium transition-all shadow-lg shadow-blue-500/25 text-sm"
          >
            Open System Settings
          </button>
          <button
            @click="checkPermission"
            :disabled="isRequesting"
            class="px-5 py-3 bg-white/10 hover:bg-white/20 text-white rounded-xl font-medium transition-all text-sm disabled:opacity-50"
          >
            <span v-if="isRequesting">...</span>
            <span v-else>Check</span>
          </button>
        </div>

        <!-- Retry request button if not yet denied by user -->
        <button
          v-if="!wasDenied"
          @click="requestPermission"
          class="w-full mt-4 px-4 py-2 text-blue-400 hover:text-blue-300 transition-colors text-sm"
        >
          Request permission again
        </button>

        <button
          @click="closeWindow"
          class="w-full mt-2 px-4 py-2 text-white/50 hover:text-white/70 transition-colors text-sm"
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
