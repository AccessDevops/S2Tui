// Maps each Language code to a circular country flag SVG bundled under
// src/assets/flags/. Vite resolves these imports to hashed asset URLs at
// build time so we only ship the 13 flags we actually reference. Source of
// the SVGs: https://github.com/HatScripts/circle-flags (MIT, pre-masked to
// a circle so no client-side cropping is needed).
//
// Country choices for ambiguous languages were confirmed with the user:
// en → GB (United Kingdom), pt → PT. Adjust here if the convention changes.

import flagGb from "../assets/flags/gb.svg";
import flagFr from "../assets/flags/fr.svg";
import flagEs from "../assets/flags/es.svg";
import flagDe from "../assets/flags/de.svg";
import flagIt from "../assets/flags/it.svg";
import flagPt from "../assets/flags/pt.svg";
import flagNl from "../assets/flags/nl.svg";
import flagJp from "../assets/flags/jp.svg";
import flagCn from "../assets/flags/cn.svg";
import flagKr from "../assets/flags/kr.svg";
import flagSa from "../assets/flags/sa.svg";
import flagIn from "../assets/flags/in.svg";
import flagPl from "../assets/flags/pl.svg";

import type { Language } from "../stores/appStore";

// `auto` deliberately has no entry: callers fall back to the status-coloured
// background when the lookup returns undefined.
export const FLAG_URLS: Partial<Record<Language, string>> = {
  en: flagGb,
  fr: flagFr,
  es: flagEs,
  de: flagDe,
  it: flagIt,
  pt: flagPt,
  nl: flagNl,
  ja: flagJp,
  zh: flagCn,
  ko: flagKr,
  ar: flagSa,
  hi: flagIn,
  pl: flagPl,
};

export function flagUrlFor(language: Language): string | undefined {
  return FLAG_URLS[language];
}

// Per-flag tone of the central area where the mic icon sits. `dark` means the
// flag's centre is bright (white/yellow/orange) so a dark mic icon reads best;
// `light` means the centre is dark (red/blue/green) so a white mic icon reads
// best. Determined by inspection of each circle-flags SVG. Defaults to `light`
// (matching the legacy white icon) for any unmapped or `auto` case.
export type FlagIconTone = "light" | "dark";

export const FLAG_ICON_TONE: Partial<Record<Language, FlagIconTone>> = {
  en: "light", // UK Union Jack: red cross over navy, white mic reads well
  fr: "dark",  // white centre stripe
  es: "dark",  // yellow centre band
  de: "light", // red centre band
  it: "dark",  // white centre stripe
  pt: "light", // crest sits over green/red boundary, mostly red behind icon
  nl: "dark",  // white centre stripe
  ja: "light", // red disc in centre
  zh: "light", // red field everywhere
  ko: "dark",  // white background with central swirl
  ar: "light", // green field
  hi: "dark",  // white centre band (chakra is small)
  pl: "dark",  // mic sits at white/red boundary, lean dark for the white half
};

export function flagIconToneFor(language: Language): FlagIconTone {
  return FLAG_ICON_TONE[language] ?? "light";
}
