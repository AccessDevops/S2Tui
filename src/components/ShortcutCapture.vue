<script setup lang="ts">
import { ref, computed, onUnmounted } from "vue";

const props = defineProps<{
  modelValue: string;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "change", value: string): void;
}>();

const isCapturing = ref(false);
const capturedKeys = ref<Set<string>>(new Set());
const error = ref<string | null>(null);

// Standard shortcuts that should not be used (common OS and app shortcuts)
const FORBIDDEN_SHORTCUTS = new Set([
  // macOS system shortcuts
  "Meta+C", "Meta+V", "Meta+X", "Meta+A", "Meta+Z", "Meta+Shift+Z",
  "Meta+S", "Meta+O", "Meta+N", "Meta+W", "Meta+Q", "Meta+P",
  "Meta+F", "Meta+G", "Meta+H", "Meta+M", "Meta+Tab",
  "Meta+Space", // Spotlight
  "Meta+Shift+3", "Meta+Shift+4", "Meta+Shift+5", // Screenshots
  "Meta+,", // Preferences
  // Windows/Linux shortcuts
  "Control+C", "Control+V", "Control+X", "Control+A", "Control+Z", "Control+Shift+Z",
  "Control+S", "Control+O", "Control+N", "Control+W", "Control+Q", "Control+P",
  "Control+F", "Control+G", "Control+H",
  "Alt+Tab", "Alt+F4",
  // Browser shortcuts
  "Meta+T", "Meta+Shift+T", "Control+T", "Control+Shift+T",
  "Meta+L", "Control+L",
  "Meta+R", "Control+R",
  // Common app shortcuts
  "Meta+B", "Meta+I", "Meta+U", // Bold, Italic, Underline
  "Control+B", "Control+I", "Control+U",
  "Escape",
]);

// Format shortcut for display (convert to readable format)
function formatShortcut(shortcut: string): string {
  return shortcut
    .replace("CommandOrControl", "⌘")
    .replace("Meta", "⌘")
    .replace("Control", "Ctrl")
    .replace("Shift", "⇧")
    .replace("Alt", "⌥")
    .replace(/\+/g, " + ");
}

// Convert key event to shortcut string
function keyEventToShortcut(e: KeyboardEvent): string {
  const parts: string[] = [];

  // Use Meta (Cmd on Mac) as primary modifier
  if (e.metaKey) parts.push("Meta");
  if (e.ctrlKey && !e.metaKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");

  // Add the actual key
  const key = e.key;
  if (!["Meta", "Control", "Alt", "Shift"].includes(key)) {
    // Normalize key names
    let normalizedKey = key;
    if (key === " ") normalizedKey = "Space";
    else if (key.length === 1) normalizedKey = key.toUpperCase();
    else if (key === "ArrowUp") normalizedKey = "Up";
    else if (key === "ArrowDown") normalizedKey = "Down";
    else if (key === "ArrowLeft") normalizedKey = "Left";
    else if (key === "ArrowRight") normalizedKey = "Right";

    parts.push(normalizedKey);
  }

  return parts.join("+");
}

// Convert captured shortcut to Tauri format
function toTauriFormat(shortcut: string): string {
  // Replace Meta with CommandOrControl for cross-platform compatibility
  return shortcut.replace("Meta", "CommandOrControl");
}

// Check if shortcut is valid (has at least one modifier + a key)
function isValidShortcut(shortcut: string): boolean {
  const parts = shortcut.split("+");
  const hasModifier = parts.some(p => ["Meta", "Control", "Alt", "Shift", "CommandOrControl"].includes(p));
  const hasKey = parts.some(p => !["Meta", "Control", "Alt", "Shift", "CommandOrControl"].includes(p));
  return hasModifier && hasKey;
}

// Check if shortcut is forbidden
function isForbiddenShortcut(shortcut: string): boolean {
  const normalized = shortcut.replace("CommandOrControl", "Meta");
  return FORBIDDEN_SHORTCUTS.has(normalized);
}

const displayValue = computed(() => formatShortcut(props.modelValue));

function startCapture() {
  isCapturing.value = true;
  error.value = null;
  capturedKeys.value.clear();
  document.addEventListener("keydown", handleKeyDown);
  document.addEventListener("keyup", handleKeyUp);
}

function stopCapture() {
  isCapturing.value = false;
  capturedKeys.value.clear();
  document.removeEventListener("keydown", handleKeyDown);
  document.removeEventListener("keyup", handleKeyUp);
}

function handleKeyDown(e: KeyboardEvent) {
  e.preventDefault();
  e.stopPropagation();

  // Escape cancels capture
  if (e.key === "Escape") {
    stopCapture();
    return;
  }

  const shortcut = keyEventToShortcut(e);

  // Only process if we have a complete shortcut (modifier + key)
  if (isValidShortcut(shortcut)) {
    // Check if forbidden
    if (isForbiddenShortcut(shortcut)) {
      error.value = "This shortcut is reserved by the system";
      return;
    }

    const tauriShortcut = toTauriFormat(shortcut);
    emit("update:modelValue", tauriShortcut);
    emit("change", tauriShortcut);
    stopCapture();
  }
}

function handleKeyUp(e: KeyboardEvent) {
  e.preventDefault();
  e.stopPropagation();
}

onUnmounted(() => {
  document.removeEventListener("keydown", handleKeyDown);
  document.removeEventListener("keyup", handleKeyUp);
});
</script>

<template>
  <div class="space-y-2">
    <button
      @click="isCapturing ? stopCapture() : startCapture()"
      :class="[
        'w-full text-left rounded-lg px-4 py-3 border transition-all font-mono text-lg',
        isCapturing
          ? 'bg-mic-listening/20 border-mic-listening text-white animate-pulse'
          : 'bg-white/10 border-white/20 text-white hover:border-white/40'
      ]"
    >
      <div class="flex items-center justify-between">
        <span v-if="isCapturing" class="text-mic-listening">
          Press a shortcut...
        </span>
        <span v-else>
          {{ displayValue }}
        </span>
        <span v-if="!isCapturing" class="text-white/40 text-sm font-sans">
          Click to change
        </span>
      </div>
    </button>

    <!-- Error message -->
    <p v-if="error" class="text-red-400 text-sm flex items-center gap-2">
      <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      {{ error }}
    </p>

    <!-- Help text -->
    <p class="text-white/40 text-xs">
      Use a combination like ⌘ + ⇧ + a key. System shortcuts are forbidden.
    </p>
  </div>
</template>
