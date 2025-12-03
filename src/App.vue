<script setup lang="ts">
import { onMounted } from "vue";
import Overlay from "./components/Overlay.vue";
import { useTauri } from "./composables/useTauri";

const { initListeners } = useTauri();

onMounted(() => {
  initListeners();
});
</script>

<template>
  <Overlay />
</template>
