import { invoke } from "@tauri-apps/api/core";

/** Offline Hunspell via Rust `spellbook` + LibreOffice ru_RU. */

export interface SpellOffer {
  word: string;
  suggestions: string[];
  start: number;
  end: number;
  replaced: string;
  fixes?: { word: string; suggestion: string }[];
}

export type SpellReadyState = "idle" | "loading" | "ready" | "error";

let readyState: SpellReadyState = "idle";
let preloadStarted = false;

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Warm the dictionary in the background (safe to call multiple times). */
export function preloadSpellcheck(): void {
  if (!isTauri() || preloadStarted) return;
  preloadStarted = true;
  readyState = "loading";
  void invoke("spell_preload")
    .then(async () => {
      // Poll until Rust finished parsing the dictionary.
      for (let i = 0; i < 40; i++) {
        const ok = await invoke<boolean>("spell_ready");
        if (ok) {
          readyState = "ready";
          return;
        }
        await new Promise((r) => window.setTimeout(r, 250));
      }
      readyState = "error";
    })
    .catch((err) => {
      readyState = "error";
      console.warn("[mote] spellcheck preload failed", err);
    });
}

export function spellcheckState(): SpellReadyState {
  return readyState;
}

/**
 * Suggest a fix for the last typed word when it's misspelled.
 * Returns null while the dictionary is loading or the word looks fine.
 */
export async function spellOffer(text: string): Promise<SpellOffer | null> {
  if (!isTauri()) return null;
  if (!/[а-яёА-ЯЁ]{3,}/.test(text)) return null;
  try {
    const next = await invoke<SpellOffer | null>("spell_offer", { text });
    if (readyState !== "ready") {
      const ok = await invoke<boolean>("spell_ready");
      readyState = ok ? "ready" : readyState === "error" ? "error" : "loading";
    }
    return next;
  } catch (err) {
    console.warn("[mote] spell_offer failed", err);
    readyState = "error";
    return null;
  }
}
