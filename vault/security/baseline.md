---
tags: [security]
status: active
updated: 2026-10-07
---

# Security baseline

## Capabilities

`src-tauri/capabilities/default.json` — минимальный allowlist:

- window show/hide/focus/always-on-top/start-dragging
- autostart enable/disable/is-enabled
- global-shortcut register/unregister

Нет произвольного FS/shell из вебвью.

## CSP

Задан restrictive CSP в `tauri.conf.json` (self + fonts + ipc).

## Secrets

- `.env*` в `.gitignore`
- DeepSeek key UI disabled in MVP
- Cursor hook `beforeSubmitPrompt` блокирует похожие на ключи вставки
- Hook `beforeShellExecution` блокирует опасные / env-exfil команды

## Next

При подключении DeepSeek — OS secure storage и scrubbing ошибок. См. skill `security`.
