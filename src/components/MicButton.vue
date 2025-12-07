<script setup lang="ts">
import { computed, ref } from "vue";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useAppStore } from "../stores/appStore";
import { useTauri } from "../composables/useTauri";
import VuMeter from "./VuMeter.vue";

const store = useAppStore();
const { startListen, stopListen } = useTauri();

const status = computed(() => store.status);
const vuLevel = computed(() => store.vuLevel);
const isDragging = ref(false);

const statusColor = computed(() => {
  switch (status.value) {
    case "listening":
      return "bg-mic-listening";
    case "processing":
      return "bg-mic-processing";
    case "error":
      return "bg-mic-error";
    default:
      return "bg-mic-idle";
  }
});

const isListening = computed(() => status.value === "listening");

async function handleClick() {
  // Don't trigger click if we just finished dragging
  if (isDragging.value) {
    isDragging.value = false;
    return;
  }

  if (status.value === "listening") {
    await stopListen();
  } else if (status.value === "idle") {
    await startListen("toggle");
  }
}

function handleMouseDown(event: MouseEvent) {
  // Only handle left click
  if (event.button !== 0) return;

  const startX = event.clientX;
  const startY = event.clientY;
  let hasMoved = false;

  const onMouseMove = (e: MouseEvent) => {
    const dx = Math.abs(e.clientX - startX);
    const dy = Math.abs(e.clientY - startY);

    // If movement exceeds threshold, start dragging
    if (!hasMoved && (dx > 5 || dy > 5)) {
      hasMoved = true;
      isDragging.value = true;

      // Clean up listeners before starting drag
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);

      // Start window dragging (don't await - it blocks until drag ends)
      getCurrentWebviewWindow().startDragging().finally(() => {
        // Keep isDragging true briefly to prevent click from firing
        setTimeout(() => {
          isDragging.value = false;
        }, 100);
      });
    }
  };

  const onMouseUp = () => {
    window.removeEventListener("mousemove", onMouseMove);
    window.removeEventListener("mouseup", onMouseUp);
    // If we didn't move, isDragging stays false and click will fire
  };

  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp);
}

function handleRightClick(e: MouseEvent) {
  e.preventDefault();
  store.toggleSettings();
}
</script>

<template>
  <div class="relative">
    <!-- VU Meter Ring -->
    <VuMeter :level="vuLevel" :active="status === 'listening'" />

    <!-- Mic Button -->
    <button
      @click="handleClick"
      @mousedown="handleMouseDown"
      @contextmenu="handleRightClick"
      :class="[
        'relative z-10 w-14 h-14 flex items-center justify-center rounded-full',
        'transition-all duration-200 ease-out',
        'hover:scale-110 active:scale-95',
        'shadow-lg hover:shadow-xl',
        'cursor-pointer',
        statusColor,
        { 'animate-pulse-fast': isListening }
      ]"
      :disabled="status === 'processing'"
    >
      <!-- Microphone Icon -->
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="w-6 h-6 text-white"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        stroke-width="2"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
        />
      </svg>

      <!-- Processing spinner -->
      <div
        v-if="status === 'processing'"
        class="absolute inset-0 flex items-center justify-center"
      >
        <svg
          class="w-8 h-8 text-white animate-spin"
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
        >
          <circle
            class="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            stroke-width="4"
          />
          <path
            class="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
          />
        </svg>
      </div>
    </button>
  </div>
</template>
