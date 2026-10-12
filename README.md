# Mote

Windows desktop companion (always-on-top overlay) built with **Tauri 2 + React + TypeScript**.

MVP chat uses a local mock engine. DeepSeek API is stubbed for a later release.

## Requirements

- Node.js 20+
- Rust via rustup
- Visual Studio 2022 Build Tools with **Desktop development with C++** (`link.exe`)
- WebView2 (usually preinstalled on Windows 10/11)

## Dev

```bash
npm install
npm run tauri dev
```

`npm run tauri` добавляет `%USERPROFILE%\.cargo\bin` в PATH (нужно, если терминал Cursor открыт до установки Rust). Новый терминал в Cursor тоже подхватывает cargo через `.vscode/settings.json`.

Hotkey: `Ctrl+Shift+M` — show/hide. Tray icon — left click toggles, right click menu.

## Cursor agents

Project skills and hooks live in `.cursor/`. Obsidian-style docs live in `vault/`.

| Skill | Role |
|---|---|
| `docs-obsidian` | Keep `vault/` notes updated |
| `bugfix` | Reproduce → fix → `vault/bugs/` |
| `security` | Secrets, CSP, Tauri capabilities |
| `mote-product` | Overlay UX + personality rules |

## Project layout

```
src/            React UI (pet overlay)
src-tauri/      Rust shell (tray, hotkey, window)
vault/          Obsidian documentation
.cursor/        skills, hooks, MCP config
```
