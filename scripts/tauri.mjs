import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, delimiter } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const cargoBin = join(homedir(), ".cargo", "bin");
const cargoExe = join(cargoBin, process.platform === "win32" ? "cargo.exe" : "cargo");

if (!existsSync(cargoExe)) {
  console.error(
    `cargo not found at ${cargoExe}\nInstall Rust from https://rustup.rs then reopen the terminal.`,
  );
  process.exit(1);
}

const pathParts = (process.env.PATH || "").split(delimiter).filter(Boolean);
if (!pathParts.some((p) => p.toLowerCase() === cargoBin.toLowerCase())) {
  process.env.PATH = `${cargoBin}${delimiter}${process.env.PATH || ""}`;
}

const tauriCli = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
if (!existsSync(tauriCli)) {
  console.error(`Tauri CLI not found at ${tauriCli}. Run: npm install`);
  process.exit(1);
}

const child = spawn(process.execPath, [tauriCli, ...process.argv.slice(2)], {
  stdio: "inherit",
  env: process.env,
});

child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
