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
    return { raw };
  }
}

const payload = await readStdinJson();
const file =
  payload.filePath ||
  payload.path ||
  payload.file ||
  payload.uri ||
  "";

const watched =
  /(^|[\\/])(src|src-tauri)([\\/]|$)/i.test(file) ||
  /tauri\.conf\.json$/i.test(file);

if (watched) {
  const message = [
    "Mote docs hook: code/shell files changed.",
    "Consider updating vault/ via docs-obsidian skill and [[00-MOC]].",
    file ? `Changed: ${file}` : "",
  ]
    .filter(Boolean)
    .join(" ");

  process.stdout.write(
    JSON.stringify({
      continue: true,
      agentMessage: message,
    }),
  );
} else {
  process.stdout.write(JSON.stringify({ continue: true }));
}
