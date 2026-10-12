import Typograf from "typograf";

export const MAX_TYPOGRAF_CHARS = 2000;

/** Shared instance — locale fixed; UTF-8 glyphs, never HTML entities. */
const tp = new Typograf({
  locale: ["ru", "en-US"],
  htmlEntity: { type: "default" },
});

/** Apply Russian typography rules. Returns null if input empty / too long. */
export function applyTypograf(text: string): string | null {
  const source = text.trim();
  if (!source || source.length > MAX_TYPOGRAF_CHARS) return null;
  return tp.execute(source);
}
