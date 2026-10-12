---
name: security
description: Enforce Mote security checklist — secrets hygiene, Tauri capabilities allowlist, CSP, and safe IPC.
---

# Security

Apply on dependency changes, new Tauri commands, settings that touch keys, shell hooks, or any network/provider work.

## Checklist

- [ ] No secrets in git (`.env`, API keys, tokens). DeepSeek key stays local / disabled in MVP.
- [ ] Tauri `capabilities/` is an allowlist — add only needed permissions.
- [ ] CSP in `tauri.conf.json` stays restrictive; justify any looseness in `vault/security/`.
- [ ] New `#[tauri::command]` validate inputs; no arbitrary shell/file access in MVP.
- [ ] Do not log API keys or chat contents that may contain secrets.
- [ ] Autostart / global shortcut / tray stay scoped to intended UX.

## When adding DeepSeek later

- Store key in OS-secure storage if available; never commit it.
- Call API from Rust or a controlled module; enforce timeouts and error scrubbing.
- Keep a kill-switch to fall back to `MockProvider`.

## Docs

Record meaningful security decisions under `vault/security/` and link from `[[00-MOC]]`.
