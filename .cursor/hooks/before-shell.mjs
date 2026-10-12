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
    return { command: raw };
  }
}

const payload = await readStdinJson();
const command = String(payload.command || payload.cmd || "");

const denyPatterns = [
  /Remove-Item\s+-Recurse\s+-Force\s+[C-Z]:\\/i,
  /rm\s+-rf\s+\/(?!\.)/i,
  /format\s+[C-Z]:/i,
  /git\s+push\s+--force/i,
  /curl.+(\.env|api[_-]?key|secret)/i,
  /type\s+\.env\b/i,
  /Get-Content\s+\.env\b/i,
  /cat\s+\.env\b/i,
];

const hit = denyPatterns.find((re) => re.test(command));

if (hit) {
  process.stdout.write(
    JSON.stringify({
      permission: "deny",
      agentMessage:
        "Blocked by Mote security hook: dangerous or secret-exfiltrating shell command.",
    }),
  );
} else {
  process.stdout.write(JSON.stringify({ permission: "allow" }));
}
