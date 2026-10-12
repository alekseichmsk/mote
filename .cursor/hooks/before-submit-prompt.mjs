import { createInterface } from "node:readline";

async function readStdinJson() {
  const lines = [];
  const rl = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of rl) lines.push(line);
  const raw = lines.join("\n").trim();
  if (!raw) return {};
  try {
    return JSON.parse(raw);
  } catch {
    return { prompt: raw };
  }
}

const payload = await readStdinJson();
const prompt = String(payload.prompt || payload.text || payload.message || "");

const secretPatterns = [
  /sk-[a-zA-Z0-9]{20,}/,
  /deepseek["'\s:=]+[a-zA-Z0-9_\-]{20,}/i,
  /api[_-]?key["'\s:=]+[a-zA-Z0-9_\-]{16,}/i,
  /-----BEGIN (RSA |OPENSSH )?PRIVATE KEY-----/,
];

const hit = secretPatterns.some((re) => re.test(prompt));

if (hit) {
  process.stdout.write(
    JSON.stringify({
      continue: false,
      userMessage:
        "Похоже, в промпте есть секрет/API-ключ. Убери ключ и повтори — для DeepSeek используй настройки приложения позже, не чат.",
    }),
  );
} else {
  process.stdout.write(JSON.stringify({ continue: true }));
}
