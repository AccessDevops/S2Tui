// Single source of truth for every language the app exposes to the user.
// Adding a new language is now a 3-line change here + (optionally) one
// flag SVG dropped in `src/assets/flags/`. The Rust side validates codes
// against the `KNOWN_WHISPER_LANGUAGES` list in `state.rs`; that list
// must stay in sync with the `code` values below.
//
// Quality tiers come from the Whisper paper's training-data hours, used
// only to set user expectations via a small UI badge:
//   - high   : ≳ 1000 h, production-grade
//   - medium : 100–1000 h, solid for common phrases (Latvian etc.)
//   - low    : < 100 h, experimental (not shipped yet)
//
// `flagCountry` is the ISO 3166-1 alpha-2 country code we map the
// language to (a single representative country — best-effort for
// languages spoken in many places). `null` means "no flag, fall back to
// status colour" (Auto, regional/stateless languages like Catalan).
//
// `flagTone` says whether the central area of the flag is bright (use a
// dark mic icon) or dark/colourful (use a white mic icon). Hand-picked
// per flag by inspection.

export type LanguageTier = "high" | "medium" | "low";
export type FlagIconTone = "light" | "dark";

export interface LanguageDef {
  code: string; // ISO 639-1 ("auto" for the special auto-detect case)
  displayName: string;
  flagCountry: string | null;
  tier: LanguageTier;
  flagTone: FlagIconTone;
}

// Order chosen to scan well in the picker: Auto first, then alphabetical
// within each tier. The cycle shortcut walks favourites in this order.
export const LANGUAGES: LanguageDef[] = [
  // -- Auto -------------------------------------------------------------
  { code: "auto", displayName: "Auto",         flagCountry: null, tier: "high", flagTone: "light" },

  // -- High tier (≳ 1000 h training) -----------------------------------
  { code: "ar",   displayName: "العربية",      flagCountry: "sa", tier: "high", flagTone: "light" },
  { code: "cs",   displayName: "Čeština",      flagCountry: "cz", tier: "high", flagTone: "light" },
  { code: "da",   displayName: "Dansk",        flagCountry: "dk", tier: "high", flagTone: "light" },
  { code: "de",   displayName: "Deutsch",      flagCountry: "de", tier: "high", flagTone: "light" },
  { code: "el",   displayName: "Ελληνικά",     flagCountry: "gr", tier: "high", flagTone: "dark" },
  { code: "en",   displayName: "English",      flagCountry: "gb", tier: "high", flagTone: "light" },
  { code: "es",   displayName: "Español",      flagCountry: "es", tier: "high", flagTone: "dark" },
  { code: "fi",   displayName: "Suomi",        flagCountry: "fi", tier: "high", flagTone: "dark" },
  { code: "fr",   displayName: "Français",     flagCountry: "fr", tier: "high", flagTone: "dark" },
  { code: "he",   displayName: "עברית",        flagCountry: "il", tier: "high", flagTone: "dark" },
  { code: "hi",   displayName: "हिन्दी",        flagCountry: "in", tier: "high", flagTone: "dark" },
  { code: "id",   displayName: "Bahasa Indonesia", flagCountry: "id", tier: "high", flagTone: "light" },
  { code: "it",   displayName: "Italiano",     flagCountry: "it", tier: "high", flagTone: "dark" },
  { code: "ja",   displayName: "日本語",        flagCountry: "jp", tier: "high", flagTone: "light" },
  { code: "ko",   displayName: "한국어",         flagCountry: "kr", tier: "high", flagTone: "dark" },
  { code: "nl",   displayName: "Nederlands",   flagCountry: "nl", tier: "high", flagTone: "dark" },
  { code: "no",   displayName: "Norsk",        flagCountry: "no", tier: "high", flagTone: "light" },
  { code: "pl",   displayName: "Polski",       flagCountry: "pl", tier: "high", flagTone: "dark" },
  { code: "pt",   displayName: "Português",    flagCountry: "pt", tier: "high", flagTone: "light" },
  { code: "ru",   displayName: "Русский",      flagCountry: "ru", tier: "high", flagTone: "dark" },
  { code: "sv",   displayName: "Svenska",      flagCountry: "se", tier: "high", flagTone: "dark" },
  { code: "tr",   displayName: "Türkçe",       flagCountry: "tr", tier: "high", flagTone: "light" },
  { code: "uk",   displayName: "Українська",   flagCountry: "ua", tier: "high", flagTone: "dark" },
  { code: "vi",   displayName: "Tiếng Việt",   flagCountry: "vn", tier: "high", flagTone: "light" },
  { code: "zh",   displayName: "中文",          flagCountry: "cn", tier: "high", flagTone: "light" },

  // -- Medium tier (100–1000 h training) -------------------------------
  { code: "az",   displayName: "Azərbaycanca", flagCountry: "az", tier: "medium", flagTone: "light" },
  { code: "be",   displayName: "Беларуская",   flagCountry: "by", tier: "medium", flagTone: "light" },
  { code: "bg",   displayName: "Български",    flagCountry: "bg", tier: "medium", flagTone: "light" },
  { code: "bs",   displayName: "Bosanski",     flagCountry: "ba", tier: "medium", flagTone: "light" },
  { code: "ca",   displayName: "Català",       flagCountry: null, tier: "medium", flagTone: "light" },
  { code: "cy",   displayName: "Cymraeg",      flagCountry: null, tier: "medium", flagTone: "light" },
  { code: "et",   displayName: "Eesti",        flagCountry: "ee", tier: "medium", flagTone: "dark" },
  { code: "eu",   displayName: "Euskara",      flagCountry: null, tier: "medium", flagTone: "light" },
  { code: "fa",   displayName: "فارسی",         flagCountry: "ir", tier: "medium", flagTone: "dark" },
  { code: "gl",   displayName: "Galego",       flagCountry: null, tier: "medium", flagTone: "light" },
  { code: "gu",   displayName: "ગુજરાતી",       flagCountry: "in", tier: "medium", flagTone: "dark" },
  { code: "hr",   displayName: "Hrvatski",     flagCountry: "hr", tier: "medium", flagTone: "dark" },
  { code: "hu",   displayName: "Magyar",       flagCountry: "hu", tier: "medium", flagTone: "dark" },
  { code: "hy",   displayName: "Հայերեն",      flagCountry: "am", tier: "medium", flagTone: "dark" },
  { code: "is",   displayName: "Íslenska",     flagCountry: "is", tier: "medium", flagTone: "light" },
  { code: "ka",   displayName: "ქართული",      flagCountry: "ge", tier: "medium", flagTone: "dark" },
  { code: "kk",   displayName: "Қазақша",      flagCountry: "kz", tier: "medium", flagTone: "light" },
  { code: "km",   displayName: "ខ្មែរ",          flagCountry: "kh", tier: "medium", flagTone: "light" },
  { code: "lo",   displayName: "ລາວ",          flagCountry: "la", tier: "medium", flagTone: "light" },
  { code: "lt",   displayName: "Lietuvių",     flagCountry: "lt", tier: "medium", flagTone: "light" },
  { code: "lv",   displayName: "Latviešu",     flagCountry: "lv", tier: "medium", flagTone: "dark" },
  { code: "mk",   displayName: "Македонски",   flagCountry: "mk", tier: "medium", flagTone: "light" },
  { code: "ml",   displayName: "മലയാളം",       flagCountry: "in", tier: "medium", flagTone: "dark" },
  { code: "mn",   displayName: "Монгол",       flagCountry: "mn", tier: "medium", flagTone: "light" },
  { code: "mr",   displayName: "मराठी",         flagCountry: "in", tier: "medium", flagTone: "dark" },
  { code: "ms",   displayName: "Bahasa Melayu", flagCountry: "my", tier: "medium", flagTone: "light" },
  { code: "mt",   displayName: "Malti",        flagCountry: "mt", tier: "medium", flagTone: "dark" },
  { code: "my",   displayName: "မြန်မာ",       flagCountry: "mm", tier: "medium", flagTone: "dark" },
  { code: "ne",   displayName: "नेपाली",        flagCountry: "np", tier: "medium", flagTone: "light" },
  { code: "ro",   displayName: "Română",       flagCountry: "ro", tier: "medium", flagTone: "dark" },
  { code: "sk",   displayName: "Slovenčina",   flagCountry: "sk", tier: "medium", flagTone: "light" },
  { code: "sl",   displayName: "Slovenščina",  flagCountry: "si", tier: "medium", flagTone: "dark" },
  { code: "sq",   displayName: "Shqip",        flagCountry: "al", tier: "medium", flagTone: "light" },
  { code: "sr",   displayName: "Српски",       flagCountry: "rs", tier: "medium", flagTone: "light" },
  { code: "sw",   displayName: "Kiswahili",    flagCountry: "tz", tier: "medium", flagTone: "dark" },
  { code: "ta",   displayName: "தமிழ்",         flagCountry: "in", tier: "medium", flagTone: "dark" },
  { code: "te",   displayName: "తెలుగు",        flagCountry: "in", tier: "medium", flagTone: "dark" },
  { code: "th",   displayName: "ไทย",          flagCountry: "th", tier: "medium", flagTone: "dark" },
  { code: "ur",   displayName: "اردو",          flagCountry: "pk", tier: "medium", flagTone: "light" },
];

// ---- Derived helpers --------------------------------------------------

const BY_CODE: Record<string, LanguageDef> = Object.fromEntries(
  LANGUAGES.map((l) => [l.code, l]),
);

export function languageDef(code: string): LanguageDef | undefined {
  return BY_CODE[code];
}

export function displayNameFor(code: string): string {
  return BY_CODE[code]?.displayName ?? code;
}

export function tierFor(code: string): LanguageTier {
  return BY_CODE[code]?.tier ?? "high";
}

export function flagCountryFor(code: string): string | null {
  return BY_CODE[code]?.flagCountry ?? null;
}

export function flagToneFor(code: string): FlagIconTone {
  return BY_CODE[code]?.flagTone ?? "light";
}

/** All language codes in display order (Auto first). */
export const ALL_LANGUAGE_CODES: string[] = LANGUAGES.map((l) => l.code);

/** Codes filtered by tier — used for sensible defaults (favorites = high). */
export function codesByTier(tier: LanguageTier): string[] {
  return LANGUAGES.filter((l) => l.tier === tier).map((l) => l.code);
}

/** Display-name lookup as a Record, kept for backward compat with older
 * call sites that imported the constant directly. New code should use
 * `displayNameFor(code)` instead. */
export const LANGUAGE_DISPLAY_NAMES: Record<string, string> =
  Object.fromEntries(LANGUAGES.map((l) => [l.code, l.displayName]));
