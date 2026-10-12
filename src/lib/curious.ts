import { invoke } from "@tauri-apps/api/core";

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type CuriousLine = {
  text: string;
  url?: string | null;
};

export type CuriousIntent =
  | { kind: "wiki"; query: string | null }
  | { kind: "nasa" }
  | { kind: "quote" }
  | { kind: "country"; name: string }
  | { kind: "book"; query: string }
  | { kind: "date" }
  | { kind: "number"; n: number };

const NOT_FOUND: CuriousLine = { text: "Ничего не нашлось." };

async function invokeLine(cmd: string, args?: Record<string, unknown>): Promise<CuriousLine> {
  if (!isTauri()) {
    return { text: "Нужен desktop Mote для любопытных фактов." };
  }
  try {
    return await invoke<CuriousLine>(cmd, args);
  } catch {
    return NOT_FOUND;
  }
}

export function fetchNumberFact(n: number, kind?: "trivia" | "math" | "year") {
  return invokeLine("curious_number_fact", { n, kind: kind ?? null });
}

export function fetchDateFact(month: number, day: number) {
  return invokeLine("curious_date_fact", { month, day });
}

export function fetchQuote() {
  return invokeLine("curious_quote");
}

export function fetchWiki(query: string) {
  return invokeLine("curious_wiki", { query });
}

export function fetchWikiRandom() {
  return invokeLine("curious_wiki_random");
}

export function fetchCountry(name: string) {
  return invokeLine("curious_country", { name });
}

export function fetchBook(query: string) {
  return invokeLine("curious_book", { query });
}

export function fetchNasaApod() {
  return invokeLine("curious_nasa_apod");
}

export async function openCuriousUrl(url: string): Promise<void> {
  if (!isTauri() || !url) return;
  await invoke("open_external_url", { url });
}

/** Arrow only for a real http(s) result — not leftover URLs on shoo/chat phrases. */
export function shouldShowGoLink(text: string, url: string | null | undefined): boolean {
  if (!url || !/^https?:\/\//i.test(url)) return false;
  const t = text.trim();
  if (!t || t === "…" || t === "Ничего не нашлось.") return false;
  if (/^(Ой — ухожу\.|Опять… за что\?|Хватит меня гонять!|Не смог улететь…)$/.test(t)) {
    return false;
  }
  if (t.startsWith("Слушаю:") || t === "Слушаю с тобой." || t === "Смотрю рядом.") return false;
  return true;
}

/**
 * Parse chat prefixes into an intent (no network).
 * Wiki: `вики путин` · `wiki …` · `w: …` · `? путин`
 */
export function parseCuriousIntent(text: string): CuriousIntent | null {
  const raw = text.trim();
  if (!raw) return null;

  const wikiPref = raw.match(/^(?:вики(?:педия)?|wiki|w:)\s*(.*)$/i);
  if (wikiPref) {
    const q = (wikiPref[1] || "").trim();
    if (!q || /^(случай|рандом|random)$/i.test(q)) {
      return { kind: "wiki", query: null };
    }
    return { kind: "wiki", query: q };
  }
  if (raw.startsWith("?")) {
    const q = raw.slice(1).trim();
    if (!q) return { kind: "wiki", query: null };
    return { kind: "wiki", query: q };
  }

  const t = raw.toLowerCase();

  if (/^(космос|апод|apod|nasa|картинка дня)$/i.test(t)) {
    return { kind: "nasa" };
  }
  if (/^(цитата|quote|скучно)$/i.test(t)) {
    return { kind: "quote" };
  }

  {
    const m = raw.match(/^(?:столица|страна|флаг)\s+(.+)$/i);
    if (m?.[1]) return { kind: "country", name: m[1].trim() };
  }

  {
    const isbn = raw.match(/\b(?:ISBN[:\s]*)?(\d{10}|\d{13})\b/i);
    if (isbn?.[1] && /^(книга|isbn)/i.test(t)) {
      return { kind: "book", query: isbn[1] };
    }
    const m = raw.match(/^(?:книга|книгу)\s+(.+)$/i);
    if (m?.[1]) return { kind: "book", query: m[1].trim() };
  }

  if (/^(что за день|факт дня)$/i.test(t)) {
    return { kind: "date" };
  }

  {
    const m = raw.match(/^факт\s+(?:про\s+)?(\d{1,6})$/i);
    if (m?.[1]) return { kind: "number", n: Number(m[1]) };
  }

  return null;
}

/**
 * Simple prefixes (not “every question → wiki” — that would break chat/mood/weather).
 */
export async function tryCuriousAsk(text: string): Promise<CuriousLine | null> {
  const intent = parseCuriousIntent(text);
  if (!intent) return null;

  switch (intent.kind) {
    case "wiki":
      return intent.query == null ? fetchWikiRandom() : fetchWiki(intent.query);
    case "nasa":
      return fetchNasaApod();
    case "quote":
      return fetchQuote();
    case "country":
      return fetchCountry(intent.name);
    case "book":
      return fetchBook(intent.query);
    case "date": {
      const now = new Date();
      return fetchDateFact(now.getMonth() + 1, now.getDate());
    }
    case "number":
      return fetchNumberFact(intent.n);
  }
}
