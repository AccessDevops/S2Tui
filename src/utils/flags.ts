// Resolves a language code to its bundled circular flag SVG URL.
// The actual language → country mapping (and the per-flag mic-icon tone)
// lives in src/utils/languages.ts; this module just owns the static SVG
// imports so Vite ships every referenced flag as a discrete asset.
//
// Source of the SVGs: https://github.com/HatScripts/circle-flags (MIT,
// pre-masked to a circle so no client-side cropping is needed).

import { flagCountryFor, flagToneFor, type FlagIconTone } from "./languages";

// Eager glob — Vite's recommended way to depend on every file in a folder
// without having to maintain a hand-rolled import list. Using `?url`
// returns the asset URL string for each match. With the project's
// `assetsInlineLimit: 0` (vite.config.ts) every SVG is shipped as its own
// hashed file under dist/assets/.
const FLAG_MODULES = import.meta.glob("../assets/flags/*.svg", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

const FLAG_URL_BY_COUNTRY: Record<string, string> = {};
for (const [path, url] of Object.entries(FLAG_MODULES)) {
  // path looks like "../assets/flags/fr.svg"
  const match = path.match(/\/([a-z-]+)\.svg$/i);
  if (match) FLAG_URL_BY_COUNTRY[match[1].toLowerCase()] = url;
}

/** Resolve the bundled flag URL for a language code, or undefined when
 *  the language has no flag (Auto, regional/stateless languages). */
export function flagUrlFor(language: string): string | undefined {
  const country = flagCountryFor(language);
  if (!country) return undefined;
  return FLAG_URL_BY_COUNTRY[country];
}

// Re-export so call sites that already import from `./flags` keep
// working without churn. New code should import directly from
// `./languages`.
export { flagToneFor, type FlagIconTone };
