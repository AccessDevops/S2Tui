<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { useTauri } from "../composables/useTauri";
import type { ModelCapabilities } from "../stores/appStore";

// ---- Component contract -----------------------------------------------
// The Settings page renders this dialog at the top level and toggles it
// via `v-model:open`. Internally we walk three states:
//
//   1. picking    — calling the native file picker
//   2. validating — invoking the Rust validator on the picked path
//   3. result     — either success (show name field + Save) or error
//                   (show simple message + <details> with technical info)
//
// The component owns its own state machine; callers only see "open or
// closed" and a "model:added" event when the user successfully saves.

const props = defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
  "model:added": [];
}>();

const { validateCustomModel, addCustomModel } = useTauri();

type State =
  | { step: "picking" }
  | { step: "validating"; path: string }
  | { step: "success"; path: string; capabilities: ModelCapabilities; warnings: ImportWarning[] }
  | { step: "error"; error: ModelCompatError; path?: string };

interface ModelCompatError {
  kind:
    | "unreadable"
    | "truncated"
    | "badMagic"
    | "notWhisper"
    | "unknownQuant"
    | "alreadyImported"
    | "insideManagedDir";
  // Variant-specific fields. Pinned union would be cleaner but the
  // Rust serde tag = "kind" emits a flat object so we tolerate the
  // spread.
  osError?: string;
  sizeBytes?: number;
  foundHex?: string;
  expectedHex?: string;
  nVocab?: number;
  nAudioCtx?: number;
  nMels?: number;
  nTextCtx?: number;
  explanation?: string;
  ftype?: number;
  existingDisplayName?: string;
}

interface ImportWarning {
  type: "nonStandardSizeClass" | "englishOnly" | "highMemoryUse";
  nAudioState?: number;
  modelSizeMb?: number;
  freeRamMb?: number;
}

interface ValidationResult {
  capabilities: ModelCapabilities;
  warnings: ImportWarning[];
}

const state = ref<State>({ step: "picking" });
const nameInput = ref("");
const saving = ref(false);
const detailsOpen = ref(false);

// ---- File-picker driver ------------------------------------------------

async function startPicking() {
  state.value = { step: "picking" };
  try {
    const picked = await openFileDialog({
      multiple: false,
      title: "Select a Whisper .bin model",
      filters: [
        { name: "Whisper model", extensions: ["bin"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (!picked) {
      // User cancelled — close modal silently.
      emit("update:open", false);
      return;
    }
    // tauri-plugin-dialog v2 returns `string` for single-file `open()`
    // calls (with `multiple: false`). Older betas returned an object;
    // we tolerate both shapes defensively.
    const path =
      typeof picked === "string"
        ? picked
        : ((picked as unknown) as { path: string }).path;
    await runValidation(path);
  } catch (err) {
    console.error("File picker failed:", err);
    state.value = {
      step: "error",
      error: {
        kind: "unreadable",
        osError: String(err),
      },
    };
  }
}

async function runValidation(path: string) {
  state.value = { step: "validating", path };
  try {
    const result = (await validateCustomModel(path)) as ValidationResult;
    nameInput.value = pathBasename(path);
    state.value = {
      step: "success",
      path,
      capabilities: result.capabilities,
      warnings: result.warnings ?? [],
    };
  } catch (err) {
    const error = (err ?? { kind: "unreadable", osError: "Unknown error" }) as ModelCompatError;
    state.value = { step: "error", error, path };
  }
}

function pathBasename(path: string): string {
  // Cross-platform basename, drops the trailing `.bin` extension if
  // present so the user sees a clean default name they can edit.
  const sep = path.includes("\\") ? "\\" : "/";
  const tail = path.split(sep).pop() ?? path;
  return tail.replace(/\.bin$/i, "");
}

async function handleSave() {
  if (state.value.step !== "success") return;
  const path = state.value.path;
  const trimmed = nameInput.value.trim();
  if (trimmed.length === 0) return;
  saving.value = true;
  try {
    await addCustomModel(trimmed, path);
    emit("model:added");
    emit("update:open", false);
  } catch (err) {
    console.error("Failed to add custom model:", err);
    // Surface as an error step. Most common case at this stage:
    // duplicate path the user re-picked between validation and save.
    const errorObj = (err ?? { kind: "unreadable", osError: "Unknown error" }) as
      | { kind: string; [k: string]: unknown }
      | ModelCompatError;
    // The backend wraps validator errors in `Compat(...)` — flatten.
    const flat: ModelCompatError =
      "kind" in errorObj && errorObj.kind === "compat" && "Compat" in errorObj
        ? (errorObj.Compat as ModelCompatError)
        : (errorObj as ModelCompatError);
    state.value = { step: "error", error: flat, path };
  } finally {
    saving.value = false;
  }
}

function handleClose() {
  emit("update:open", false);
}

// ---- Open/close lifecycle ---------------------------------------------

watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      // Reset internal state every time the dialog opens.
      detailsOpen.value = false;
      nameInput.value = "";
      saving.value = false;
      // Kick the picker on the next tick so the modal is visible
      // (with a "loading file picker..." placeholder) before the
      // native dialog steals focus.
      startPicking();
    }
  },
  { immediate: false },
);

// ---- Computed labels ---------------------------------------------------

const errorTitle = computed(() => {
  if (state.value.step !== "error") return "";
  switch (state.value.error.kind) {
    case "unreadable":
      return "Couldn't open this file";
    case "truncated":
      return "File too short to be a model";
    case "badMagic":
      return "This file isn't a GGML model";
    case "notWhisper":
      return "GGML model — but not Whisper";
    case "unknownQuant":
      return "Unsupported quantisation variant";
    case "alreadyImported":
      return "Already imported";
    case "insideManagedDir":
      return "Built-in model location";
  }
  return "Couldn't import this file";
});

const errorMessage = computed(() => {
  if (state.value.step !== "error") return "";
  const e = state.value.error;
  switch (e.kind) {
    case "unreadable":
      return "Check that you have read permission for this file. On macOS, files outside your home directory may need explicit permission.";
    case "truncated":
      return "This file is too short to be a complete model. It may have been corrupted during download.";
    case "badMagic":
      return "Whisper needs a binary file produced by whisper.cpp's converter. GGUF files (.gguf) are LLaMA's newer format and aren't compatible.";
    case "notWhisper":
      return e.explanation ?? "This GGML file isn't a Whisper model.";
    case "unknownQuant":
      return "This file looks like a Whisper model but uses a quantisation variant we don't support yet. Try a model quantised with q5_0, q5_1, q4_K, q5_K, q6_K, f16, or f32.";
    case "alreadyImported":
      return `This file is already imported as "${e.existingDisplayName ?? "an existing model"}".`;
    case "insideManagedDir":
      return "This file is inside the app's built-in models folder. Built-ins are managed automatically — you don't need to import them.";
  }
  return "";
});

const successSubtitle = computed(() => {
  if (state.value.step !== "success") return "";
  const c = state.value.capabilities;
  const sizeMb = Math.round(c.fileSizeBytes / 1_048_576);
  const mode = c.isMultilingual ? "multilingual" : "English-only";
  return `${c.sizeClass} · ${c.quantLabel} · ${mode} · ${sizeMb} MB`;
});

const englishOnlyHint = computed(() => {
  if (state.value.step !== "success") return false;
  return state.value.warnings.some((w) => w.type === "englishOnly");
});

const nonStandardSizeHint = computed(() => {
  if (state.value.step !== "success") return null;
  return state.value.warnings.find((w) => w.type === "nonStandardSizeClass");
});
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    @click.self="handleClose"
  >
    <div
      class="w-[480px] max-w-[92vw] rounded-2xl bg-zinc-900 border border-white/10 shadow-2xl overflow-hidden"
    >
      <!-- Header -->
      <div class="px-6 py-4 border-b border-white/10 flex items-center justify-between">
        <h2 class="text-white text-lg font-semibold">Add custom Whisper model</h2>
        <button
          type="button"
          @click="handleClose"
          class="text-white/40 hover:text-white/80 transition-colors"
          aria-label="Close"
        >
          <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Body -->
      <div class="px-6 py-5">
        <!-- State: picking (waiting for native dialog) -->
        <div v-if="state.step === 'picking'" class="text-white/60 text-sm py-6 text-center">
          Opening file picker…
        </div>

        <!-- State: validating -->
        <div v-else-if="state.step === 'validating'" class="text-white/70 text-sm py-6 text-center">
          <svg class="w-5 h-5 animate-spin inline-block mr-2 -mt-1" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
          Checking compatibility…
        </div>

        <!-- State: success → name input -->
        <div v-else-if="state.step === 'success'" class="space-y-4">
          <div>
            <label class="block text-white text-sm font-medium mb-1.5">Model name</label>
            <input
              v-model="nameInput"
              type="text"
              maxlength="80"
              class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white placeholder:text-white/30 focus:outline-none focus:border-blue-500"
              placeholder="My fine-tuned model"
              @keyup.enter="handleSave"
            />
            <p class="text-white/50 text-xs mt-1.5">
              Detected: {{ successSubtitle }}
            </p>
            <p v-if="englishOnlyHint" class="text-amber-300/80 text-xs mt-1.5">
              ⚠ This is an English-only variant. Other languages will be disabled
              for this model in the favorites picker.
            </p>
            <p v-if="nonStandardSizeHint" class="text-amber-300/80 text-xs mt-1.5">
              ⚠ Non-standard model size (n_audio_state={{ nonStandardSizeHint.nAudioState }}).
              We'll let it through but can't promise stable performance.
            </p>
          </div>
        </div>

        <!-- State: error -->
        <div v-else-if="state.step === 'error'" class="space-y-3">
          <div class="flex items-start gap-3">
            <div class="w-8 h-8 rounded-full bg-red-500/20 flex items-center justify-center flex-shrink-0 mt-0.5">
              <svg class="w-5 h-5 text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
            </div>
            <div class="flex-1">
              <p class="text-white font-medium">{{ errorTitle }}</p>
              <p class="text-white/60 text-sm mt-1 leading-relaxed">{{ errorMessage }}</p>
            </div>
          </div>

          <details
            class="rounded-lg bg-white/5 border border-white/10 px-3 py-2 text-xs text-white/60"
            :open="detailsOpen"
          >
            <summary class="cursor-pointer text-white/80 select-none outline-none">
              Show technical details
            </summary>
            <dl class="mt-2 space-y-1 font-mono">
              <div v-if="state.path" class="flex gap-2">
                <dt class="text-white/40 shrink-0">path</dt>
                <dd class="text-white/80 break-all">{{ state.path }}</dd>
              </div>
              <div class="flex gap-2">
                <dt class="text-white/40 shrink-0">kind</dt>
                <dd class="text-white/80">{{ state.error.kind }}</dd>
              </div>
              <div v-if="state.error.foundHex" class="flex gap-2">
                <dt class="text-white/40 shrink-0">magic</dt>
                <dd class="text-white/80">{{ state.error.foundHex }} (expected {{ state.error.expectedHex }})</dd>
              </div>
              <div v-if="state.error.kind === 'notWhisper'" class="flex gap-2">
                <dt class="text-white/40 shrink-0">hparams</dt>
                <dd class="text-white/80">
                  n_vocab={{ state.error.nVocab }} ·
                  n_audio_ctx={{ state.error.nAudioCtx }} ·
                  n_text_ctx={{ state.error.nTextCtx }} ·
                  n_mels={{ state.error.nMels }}
                </dd>
              </div>
              <div v-if="state.error.kind === 'unknownQuant'" class="flex gap-2">
                <dt class="text-white/40 shrink-0">ftype</dt>
                <dd class="text-white/80">{{ state.error.ftype }}</dd>
              </div>
              <div v-if="state.error.kind === 'truncated'" class="flex gap-2">
                <dt class="text-white/40 shrink-0">size</dt>
                <dd class="text-white/80">{{ state.error.sizeBytes }} bytes</dd>
              </div>
              <div v-if="state.error.osError" class="flex gap-2">
                <dt class="text-white/40 shrink-0">os</dt>
                <dd class="text-white/80 break-all">{{ state.error.osError }}</dd>
              </div>
            </dl>
            <p class="mt-3 text-white/50 leading-relaxed">
              <strong class="text-white/70">Common pitfalls:</strong> GGUF (.gguf) files
              are LLaMA, not Whisper. PyTorch (.pt) checkpoints need conversion via
              whisper.cpp's <code class="text-white/70">models/convert-pt-to-ggml.py</code>.
              HuggingFace .safetensors files aren't supported.
            </p>
          </details>
        </div>
      </div>

      <!-- Footer -->
      <div class="px-6 py-3 border-t border-white/10 flex justify-end gap-2 bg-black/20">
        <button
          v-if="state.step === 'success'"
          type="button"
          @click="handleClose"
          class="px-4 py-2 rounded-lg bg-white/5 hover:bg-white/10 text-white/70 text-sm transition-colors"
        >
          Cancel
        </button>
        <button
          v-if="state.step === 'success'"
          type="button"
          @click="handleSave"
          :disabled="saving || nameInput.trim().length === 0"
          class="px-4 py-2 rounded-lg bg-blue-500 hover:bg-blue-600 text-white text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {{ saving ? "Saving…" : "Save" }}
        </button>
        <button
          v-else-if="state.step === 'error'"
          type="button"
          @click="handleClose"
          class="px-4 py-2 rounded-lg bg-white/10 hover:bg-white/20 text-white text-sm transition-colors"
        >
          OK
        </button>
      </div>
    </div>
  </div>
</template>
