#!/usr/bin/env node
/**
 * Build latest.json for GitHub Releases updater.
 *
 * Usage:
 *   node scripts/make-latest-json.mjs \
 *     --version 0.1.1 \
 *     --setup path/to/Mote_0.1.1_x64-setup.exe \
 *     --sig path/to/Mote_0.1.1_x64-setup.exe.sig \
 *     --owner OWNER --repo REPO \
 *     --out latest.json
 */
import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  if (i >= 0 && process.argv[i + 1]) return process.argv[i + 1];
  return fallback;
}

const version = arg("version");
const setup = arg("setup");
const sigPath = arg("sig", setup ? `${setup}.sig` : undefined);
const owner = arg("owner", "alekseichmsk");
const repo = arg("repo", "mote");
const out = arg("out", "latest.json");
const notes = arg("notes", "");

if (!version || !setup || !sigPath) {
  console.error(
    "Need --version, --setup, and --sig (or setup.exe.sig next to --setup)",
  );
  process.exit(1);
}

const signature = readFileSync(sigPath, "utf8").trim();
const fileName = basename(setup);
const url = `https://github.com/${owner}/${repo}/releases/download/v${version.replace(/^v/, "")}/${fileName}`;

const json = {
  version: version.replace(/^v/, ""),
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url,
    },
  },
};

writeFileSync(out, `${JSON.stringify(json, null, 2)}\n`, "utf8");
console.log(`Wrote ${out}`);
console.log(`url: ${url}`);
