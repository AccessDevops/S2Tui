<script setup lang="ts">
import { computed, ref } from "vue";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import MicButton from "./MicButton.vue";
import { useAppStore } from "../stores/appStore";
import { openSettings } from "../composables/useTauri";

const store = useAppStore();
const showCopyNotification = computed(() => store.showCopyNotification);

const isDragging = ref(false);

function handleMouseDown(event: MouseEvent) {
  // Only handle left click
  if (event.button !== 0) return;

  // Don't start drag if clicking on interactive elements
  const target = event.target as HTMLElement;
  if (target.tagName === 'BUTTON' || target.closest('button')) {
    return;
  }

  const startX = event.clientX;
  const startY = event.clientY;
  let hasMoved = false;

  const onMouseMove = (e: MouseEvent) => {
    const dx = Math.abs(e.clientX - startX);
    const dy = Math.abs(e.clientY - startY);

    // Only start dragging if movement exceeds threshold
    if (!hasMoved && (dx > 5 || dy > 5)) {
      hasMoved = true;
      isDragging.value = true;

      // Clean up listeners before starting drag
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);

      // Start window dragging (don't await - it blocks until drag ends)
      getCurrentWebviewWindow().startDragging().finally(() => {
        isDragging.value = false;
      });
    }
  };

  const onMouseUp = () => {
    window.removeEventListener("mousemove", onMouseMove);
    window.removeEventListener("mouseup", onMouseUp);
    isDragging.value = false;
  };

  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp);
}
</script>

<template>
  <div class="fixed inset-0 flex items-center justify-center pointer-events-none">
    <!-- Windows fix: minimal alpha background on clickable area to receive events on transparent windows -->
    <div
      class="relative pointer-events-auto p-4 -m-4 rounded-full"
      style="background: rgba(0,0,0,0.01)"
      @mousedown="handleMouseDown"
    >
      <!-- Mic Button -->
      <MicButton />

      <!-- Settings Button - bottom center -->
      <button
        @click="openSettings"
        class="absolute z-20 w-6 h-6 rounded-full bg-white/20 hover:bg-white/30 flex items-center justify-center text-white/70 hover:text-white transition-all bottom-1 left-1/2 -translate-x-1/2"
        title="Settings"
      >
        <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
          />
        </svg>
      </button>

      <!-- Copy Notification Popover (centered on mic button) -->
      <Transition
        enter-active-class="transition-all duration-300 ease-out"
        enter-from-class="opacity-0 scale-75"
        enter-to-class="opacity-100 scale-100"
        leave-active-class="transition-all duration-200 ease-in"
        leave-from-class="opacity-100 scale-100"
        leave-to-class="opacity-0 scale-75"
      >
        <div
          v-if="showCopyNotification"
          class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-30 whitespace-nowrap"
        >
          <div class="flex items-center gap-1.5 px-2 py-1.5 rounded-md bg-green-500/90 text-white text-xs font-medium shadow-lg backdrop-blur-sm">
            <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
            </svg>
            Copied
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>
