<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  level: number;
  active: boolean;
}>();

// Circle parameters
const radius = 32;
const circumference = 2 * Math.PI * radius;

// Calculate stroke-dashoffset based on level (0-1)
const strokeOffset = computed(() => {
  const progress = props.active ? props.level : 0;
  return circumference * (1 - progress);
});

const opacity = computed(() => (props.active ? 0.8 : 0.2));
</script>

<template>
  <svg
    class="absolute inset-0 w-full h-full -rotate-90 pointer-events-none"
    viewBox="0 0 72 72"
  >
    <!-- Background circle -->
    <circle
      cx="36"
      cy="36"
      :r="radius"
      fill="none"
      stroke="currentColor"
      stroke-width="4"
      class="text-gray-300/30"
    />

    <!-- VU meter circle -->
    <circle
      cx="36"
      cy="36"
      :r="radius"
      fill="none"
      stroke="currentColor"
      stroke-width="4"
      stroke-linecap="round"
      :stroke-dasharray="circumference"
      :stroke-dashoffset="strokeOffset"
      :style="{ opacity, transition: 'stroke-dashoffset 0.1s ease-out, opacity 0.3s' }"
      class="text-mic-listening"
    />
  </svg>
</template>
