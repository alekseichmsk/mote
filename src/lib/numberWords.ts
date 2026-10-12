/** Convert digit runs in text to Russian words (прописью). */

const ONES_M = [
  "ноль",
  "один",
  "два",
  "три",
  "четыре",
  "пять",
  "шесть",
  "семь",
  "восемь",
  "девять",
] as const;

const ONES_F = [
  "ноль",
  "одна",
  "две",
  "три",
  "четыре",
  "пять",
  "шесть",
  "семь",
  "восемь",
  "девять",
] as const;

const TEENS = [
  "десять",
  "одиннадцать",
  "двенадцать",
  "тринадцать",
  "четырнадцать",
  "пятнадцать",
  "шестнадцать",
  "семнадцать",
  "восемнадцать",
  "девятнадцать",
] as const;

const TENS = [
  "",
  "",
  "двадцать",
  "тридцать",
  "сорок",
  "пятьдесят",
  "шестьдесят",
  "семьдесят",
  "восемьдесят",
  "девяносто",
] as const;

const HUNDREDS = [
  "",
  "сто",
  "двести",
  "триста",
  "четыреста",
  "пятьсот",
  "шестьсот",
  "семьсот",
  "восемьсот",
  "девятьсот",
] as const;

type Gender = "m" | "f";

function triadToWords(n: number, gender: Gender): string {
  if (n <= 0 || n > 999) return "";
  const parts: string[] = [];
  const h = Math.floor(n / 100);
  const rem = n % 100;
  if (h) parts.push(HUNDREDS[h]);
  if (rem >= 10 && rem <= 19) {
    parts.push(TEENS[rem - 10]);
  } else {
    const t = Math.floor(rem / 10);
    const o = rem % 10;
    if (t) parts.push(TENS[t]);
    if (o) parts.push(gender === "f" ? ONES_F[o] : ONES_M[o]);
  }
  return parts.join(" ");
}

function pluralForm(n: number, one: string, few: string, many: string): string {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return one;
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return few;
  return many;
}

/** Integer 0…999_999_999_999 → Russian words. */
export function numberToRussianWords(value: number | bigint): string {
  let n = typeof value === "bigint" ? value : BigInt(Math.trunc(value));
  if (n < 0n) return `минус ${numberToRussianWords(-n)}`;
  if (n === 0n) return "ноль";

  const scales: { div: bigint; gender: Gender; one: string; few: string; many: string }[] = [
    { div: 1_000_000_000n, gender: "m", one: "миллиард", few: "миллиарда", many: "миллиардов" },
    { div: 1_000_000n, gender: "m", one: "миллион", few: "миллиона", many: "миллионов" },
    { div: 1_000n, gender: "f", one: "тысяча", few: "тысячи", many: "тысяч" },
  ];

  const parts: string[] = [];
  for (const scale of scales) {
    if (n >= scale.div) {
      const chunk = Number(n / scale.div);
      n %= scale.div;
      const words = triadToWords(chunk, scale.gender);
      if (words) {
        parts.push(`${words} ${pluralForm(chunk, scale.one, scale.few, scale.many)}`);
      }
    }
  }
  if (n > 0n) {
    parts.push(triadToWords(Number(n), "m"));
  }
  return parts.filter(Boolean).join(" ");
}

const DIGIT_RUN = /\d+/g;

export interface DigitOffer {
  /** Full text with every digit run replaced by words. */
  replaced: string;
  /** Short preview for the bubble (first replacement or summary). */
  preview: string;
  /** How many digit runs were found. */
  count: number;
}

/** Build a replace-with-words offer, or null if there are no digits / number too big. */
export function digitWordsOffer(text: string): DigitOffer | null {
  const matches = [...text.matchAll(DIGIT_RUN)];
  if (matches.length === 0) return null;

  const previews: string[] = [];
  let replaced = "";
  let last = 0;

  for (const m of matches) {
    const raw = m[0];
    const start = m.index ?? 0;
    if (raw.length > 12) return null;
    const words = numberToRussianWords(BigInt(raw));
    replaced += text.slice(last, start) + words;
    last = start + raw.length;
    if (previews.length < 2) previews.push(words);
  }
  replaced += text.slice(last);

  if (replaced === text) return null;

  const preview =
    matches.length === 1
      ? previews[0]
      : previews.length < matches.length
        ? `${previews.join(", ")}…`
        : previews.join(", ");

  return { replaced, preview, count: matches.length };
}
