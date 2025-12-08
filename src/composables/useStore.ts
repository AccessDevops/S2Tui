import { load, Store } from "@tauri-apps/plugin-store";
import type { Settings, HistoryEntry, ModelId } from "../stores/appStore";

const STORE_FILE = "settings.json";

let store: Store | null = null;

export interface PersistedSettings extends Settings {
  history: HistoryEntry[];
  vulkanWarningDismissed: boolean;
  welcomeDismissed: boolean;
}

const defaultSettings: PersistedSettings = {
  language: "auto",
  model: "large-v3-turbo",
  autoCopy: true,
  shortcut: "CommandOrControl+Shift+Space",
  history: [],
  vulkanWarningDismissed: false,
  welcomeDismissed: false,
};

async function getStore(): Promise<Store> {
  if (!store) {
    store = await load(STORE_FILE, {
      defaults: { settings: defaultSettings },
      autoSave: 300,
    });
  }
  return store;
}

export async function loadSettings(): Promise<PersistedSettings> {
  try {
    const s = await getStore();
    const settings = await s.get<PersistedSettings>("settings");
    if (settings) {
      return { ...defaultSettings, ...settings };
    }
  } catch (error) {
    console.error("Failed to load settings:", error);
  }
  return defaultSettings;
}

export async function saveSettings(settings: Partial<PersistedSettings>): Promise<void> {
  try {
    const s = await getStore();
    const current = await loadSettings();
    const updated = { ...current, ...settings };
    await s.set("settings", updated);
    await s.save();
  } catch (error) {
    console.error("Failed to save settings:", error);
  }
}

export async function loadHistory(): Promise<HistoryEntry[]> {
  const settings = await loadSettings();
  return settings.history || [];
}

export async function saveHistory(history: HistoryEntry[]): Promise<void> {
  await saveSettings({ history: history.slice(0, 20) });
}

export async function addHistoryEntry(text: string, modelId?: string, durationMs?: number): Promise<HistoryEntry> {
  const entry: HistoryEntry = {
    id: Date.now().toString(),
    text,
    timestamp: Date.now(),
    modelId: modelId as ModelId | undefined,
    durationMs,
  };
  const settings = await loadSettings();
  const history = [entry, ...(settings.history || [])].slice(0, 20);
  await saveSettings({ history });
  return entry;
}

export async function clearHistory(): Promise<void> {
  await saveSettings({ history: [] });
}
