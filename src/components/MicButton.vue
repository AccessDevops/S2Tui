<script setup lang="ts">
import { computed, ref } from "vue";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useAppStore } from "../stores/appStore";
import { useTauri, openSettings } from "../composables/useTauri";
import { flagToneFor, flagUrlFor } from "../utils/flags";
import VuMeter from "./VuMeter.vue";

const store = useAppStore();
const { startListen, stopListen } = useTauri();

const status = computed(() => store.status);
const vuLevel = computed(() => store.vuLevel);
const isDragging = ref(false);

// When a specific language is active we paint the matching flag in an inner
// disc inset by 3px so the surrounding `bg-mic-{state}` colour acts as a
// thick visible ring. This keeps the green-while-listening (and blue/red)
// indicator clearly readable even when a flag covers the centre.
const flagUrl = computed(() => flagUrlFor(store.settings.language));
const hasFlag = computed(() => flagUrl.value !== undefined);

// Bright-flag detection drives the mic icon colour: white on dark/colourful
// flags, near-black on bright flags so the icon never disappears (e.g. the
// French white centre stripe).
const flagTone = computed(() => flagToneFor(store.settings.language));
const iconColor = computed(() => {
  if (!hasFlag.value) return "text-white";
  return flagTone.value === "dark" ? "text-gray-900" : "text-white";
});

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

// ---- Model download state ---------------------------------------------
// Driven by the `modelDownload` slice in appStore. While at least one
// required model is missing we paint a blue progress ring around the
// button, dim the flag/mic icon, intercept clicks (swap the recording
// attempt for a toast), and surface a hover tooltip with the byte counts.
const isModelDownloading = computed(() => store.modelDownload.active);
const downloadPercent = computed(() => store.modelDownloadCumulativePercent);
const hasDownloadError = computed(() =>
  store.modelDownload.items.some((i) => i.status === "error"),
);

function formatBytes(n: number): string {
  if (n <= 0) return "0 MB";
  const mb = n / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}

const downloadTooltip = computed(() => {
  if (!isModelDownloading.value) return "";
  if (hasDownloadError.value) {
    const failed = store.modelDownload.items.find((i) => i.status === "error");
    return `Download failed${failed?.errorMessage ? `: ${failed.errorMessage}` : ""} — open Settings → Models to retry`;
  }
  const items = store.modelDownload.items;
  const done = items.reduce((s, i) => s + i.bytesReceived, 0);
  const total = items.reduce((s, i) => s + i.sizeBytes, 0);
  return `Downloading speech models — ${formatBytes(done)} / ${formatBytes(total)} (${downloadPercent.value}%)`;
});

// SVG ring geometry — kept here so the template stays declarative. The
// circle sits just outside the 56 px button (radius 30, drawn on a 64 px
// box). Stroke uses dasharray = circumference; offset shrinks as percent
// climbs, so the filled arc grows clockwise from 12 o'clock.
const RING_RADIUS = 30;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;
const ringDashOffset = computed(
  () => RING_CIRCUMFERENCE * (1 - downloadPercent.value / 100),
);

async function handleClick() {
  // Don't trigger click if we just finished dragging
  if (isDragging.value) {
    isDragging.value = false;
    return;
  }

  // Soft-lock the mic while models are still downloading — clicking now
  // would either start a recording with no model loaded or, worse, try to
  // load a `.partial` file. Surface what's going on instead.
  if (isModelDownloading.value) {
    if (hasDownloadError.value) {
      store.showToggleNotification("Download failed — open Settings");
    } else {
      store.showToggleNotification(
        `Downloading models — ${downloadPercent.value}%`,
      );
    }
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
  openSettings();
}
</script>

<template>
  <div class="relative">
    <!-- VU Meter Ring -->
    <VuMeter :level="vuLevel" :active="status === 'listening'" />

    <!-- Download progress ring. Sits in the same .relative parent as the
         button so it overlays it without affecting layout. Hidden once all
         downloads complete. The grey track + blue arc give a clear "this is
         not the listening VU meter" cue. -->
    <svg
      v-if="isModelDownloading"
      class="absolute inset-0 m-auto pointer-events-none z-20"
      :class="hasDownloadError ? 'text-red-400' : 'text-blue-400'"
      width="64"
      height="64"
      viewBox="0 0 64 64"
    >
      <circle
        cx="32"
        cy="32"
        :r="RING_RADIUS"
        fill="none"
        stroke="currentColor"
        stroke-opacity="0.18"
        stroke-width="3"
      />
      <circle
        cx="32"
        cy="32"
        :r="RING_RADIUS"
        fill="none"
        stroke="currentColor"
        stroke-width="3"
        stroke-linecap="round"
        :stroke-dasharray="RING_CIRCUMFERENCE"
        :stroke-dashoffset="ringDashOffset"
        transform="rotate(-90 32 32)"
        style="transition: stroke-dashoffset 200ms linear"
      />
    </svg>

    <!-- Mic Button -->
    <button
      @click="handleClick"
      @mousedown="handleMouseDown"
      @contextmenu="handleRightClick"
      :title="downloadTooltip || undefined"
      :class="[
        'relative z-10 w-14 h-14 flex items-center justify-center rounded-full overflow-hidden',
        'transition-all duration-200 ease-out',
        'shadow-lg hover:shadow-xl',
        // While downloading, swap the playful hover/active scale for a
        // not-allowed cursor — the click is intercepted to show a toast
        // instead of starting a doomed recording.
        isModelDownloading
          ? 'cursor-not-allowed'
          : 'hover:scale-110 active:scale-95 cursor-pointer',
        // The status fill is always the button background. When a flag is
        // active, a smaller inner disc sits on top so the status colour is
        // visible as a thick ring around the flag.
        statusColor,
        { 'animate-pulse-fast': isListening && !isModelDownloading }
      ]"
      :disabled="status === 'processing'"
    >
      <!-- Inner flag disc (only when language has a flag). Inset by 4px on
           every side gives a 4px coloured ring (the button's bg-mic-{state})
           that stays clearly visible while listening/processing/error.
           Dimmed to 50% while downloading: identity preserved, "not ready"
           signal clear. -->
      <div
        v-if="hasFlag"
        :class="[
          'absolute inset-[4px] rounded-full overflow-hidden bg-cover bg-center pointer-events-none transition-opacity',
          isModelDownloading ? 'opacity-50' : '',
        ]"
        :style="{ backgroundImage: `url(${flagUrl})` }"
      >
        <!-- Subtle dark wash only on bright flags, where the white mic icon
             would otherwise wash out. Dropped for `dark`-tone flags so the
             colour stays vibrant. -->
        <div
          v-if="flagTone === 'light'"
          class="absolute inset-0 bg-black/20"
        ></div>
      </div>

      <!-- Microphone Icon -->
      <svg
        xmlns="http://www.w3.org/2000/svg"
        :class="[
          'relative z-10 w-6 h-6 transition-colors',
          iconColor,
          isModelDownloading ? 'opacity-40' : '',
        ]"
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
