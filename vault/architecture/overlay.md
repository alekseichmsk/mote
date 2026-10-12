---
tags: [architecture]
status: active
updated: 2026-10-12
---

# Overlay architecture

## Window

Tauri 2 main window:

- `decorations: false`
- `transparent: true`
- `shadow: false` (критично на Windows)
- `alwaysOnTop: true`
- `skipTaskbar: true`

## Shell features

- System tray: show / hide / settings / check update / install update / quit
- Settings window (`settings` label): frameless/transparent, custom chrome; pet name + weather city; opened from tray only
- Global shortcut `Ctrl+Shift+M`
- Frontend drag via `getCurrentWindow().startDragging()`
- Settings: always-on-top toggle, autostart plugin
- Updater: `tauri-plugin-updater` + GitHub Releases (`latest.json`); see [[design/auto-update]]
- Presence (`watcher.rs`): eyes follow cursor by default (body stays); hop to non-input clicks / maximize; caret dock while typing; video dock beside player
- Spellcheck (`spellcheck.rs` + `field_text.rs`): Windows Spell Checking API (`ISpellChecker` / `ru-RU`) first, Hunspell LibreOffice `ru_RU` fallback; while typing, reads only the focused fill control (`Edit` / ValuePattern — not page/screen text) and emits `mote://spell-offer`; replace prefers UIA / Ctrl+A+paste
- Curious (`curious.rs` + `src/lib/curious.ts`): free APIs — Numbers (digit plaque fact), Quotable (daily/idle), Wikipedia / Countries / Open Library / NASA APOD via chat intents; `open_external_url` for «открой»
- Video detect: window title markers + active-tab URL via UI Automation address bar (`browser_url.rs`, cached ~1.2s)
- Hit-test (`hit_regions.rs`): `set_ignore_cursor_events` outside pet/bubble/menu so transparent chrome does not block desktop UI
- Motion: ~144 FPS float lerp only (no teleport); faster glide if far, soft ease nearby
- Music (`media_session.rs`): Windows SMTC poll ~1s; `mote://music` when Playing and not `MediaPlaybackType::Video`; bubble phrase, no dock

## Frontend

React UI in `src/`: avatar, speech bubble, chat panel, settings stub for DeepSeek.

## Chat

`ChatEngine` + `MockProvider` (live) + `DeepSeekProvider` (stub).

Связано: [[product/vision]], [[security/baseline]].
