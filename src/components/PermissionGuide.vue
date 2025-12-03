<script setup lang="ts">
import { computed } from "vue";
import { useAppStore } from "../stores/appStore";
import { useTauri } from "../composables/useTauri";

const store = useAppStore();
const { checkPermissions } = useTauri();

const show = computed(() => store.showPermissionGuide);
const permissionType = computed(() => store.permissionType);

const title = computed(() => {
  if (permissionType.value === "microphone") {
    return "Microphone access required";
  }
  return "Permission required";
});

const description = computed(() => {
  if (permissionType.value === "microphone") {
    return "S2Tui needs access to your microphone for speech recognition.";
  }
  return "";
});

const steps = computed(() => {
  if (permissionType.value === "microphone") {
    return [
      "Open System Preferences > Security & Privacy",
      "Click on the Privacy tab",
      "Select Microphone from the list",
      "Check the box next to S2Tui",
    ];
  }
  return [];
});

function handleGrant() {
  // Open System Preferences to Privacy settings
  window.open("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
}

async function handleCheck() {
  const perms = await checkPermissions();
  if (permissionType.value === "microphone" && perms.microphone) {
    store.closePermissionGuide();
  }
}

function handleClose() {
  store.closePermissionGuide();
}
</script>

<template>
  <Transition
    enter-active-class="transition-all duration-300 ease-out"
    enter-from-class="opacity-0 scale-95"
    enter-to-class="opacity-100 scale-100"
    leave-active-class="transition-all duration-200 ease-in"
    leave-from-class="opacity-100 scale-100"
    leave-to-class="opacity-0 scale-95"
  >
    <div
      v-if="show"
      class="fixed inset-0 flex items-center justify-center z-50 bg-black/50 backdrop-blur-sm"
    >
      <div class="glass rounded-xl p-6 w-96 shadow-2xl border border-white/20">
        <!-- Header -->
        <div class="flex items-center gap-3 mb-4">
          <div class="w-10 h-10 rounded-full bg-amber-500/20 flex items-center justify-center">
            <svg
              class="w-5 h-5 text-amber-400"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
              />
            </svg>
          </div>
          <div>
            <h3 class="text-white font-semibold text-lg">{{ title }}</h3>
            <p class="text-white/60 text-sm">{{ description }}</p>
          </div>
        </div>

        <!-- Steps -->
        <div class="bg-white/5 rounded-lg p-4 mb-4">
          <p class="text-white/70 text-xs uppercase tracking-wide mb-2">Steps to follow</p>
          <ol class="space-y-2">
            <li
              v-for="(step, index) in steps"
              :key="index"
              class="flex items-start gap-2 text-white/80 text-sm"
            >
              <span class="w-5 h-5 rounded-full bg-white/20 flex items-center justify-center text-xs flex-shrink-0 mt-0.5">
                {{ index + 1 }}
              </span>
              {{ step }}
            </li>
          </ol>
        </div>

        <!-- Actions -->
        <div class="flex gap-2">
          <button
            @click="handleGrant"
            class="flex-1 px-4 py-2 bg-mic-listening hover:bg-mic-listening/80 text-white rounded-lg font-medium transition-colors text-sm"
          >
            Open settings
          </button>
          <button
            @click="handleCheck"
            class="px-4 py-2 bg-white/10 hover:bg-white/20 text-white rounded-lg font-medium transition-colors text-sm"
          >
            Check
          </button>
          <button
            @click="handleClose"
            class="px-4 py-2 text-white/60 hover:text-white/80 transition-colors text-sm"
          >
            Later
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>
