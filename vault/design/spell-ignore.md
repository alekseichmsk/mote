---
tags: [design, spell, layout]
status: active
updated: 2026-10-12
---

# Spell ignore + intentional English

## Цель

Обычный пользователь может оставить «хмм», аббревиатуры (`PDF`, `OK`) и свой EN в смешанном тексте — Mote не меняет их сам.

## Поведение

1. Plaque → **Оставить** → слова в `%APPDATA%\Mote\spell_ignore.json`.
2. Spell / layout_auto / кнопка «Раскладка» пропускают ignore.
3. **ALL-CAPS** латиница (2+) не авто-конвертируется.
4. en_US по-прежнему защищает `hello` / `bot` (≥3 в mixed).
5. «Раскладка» / force: только Latin→RU по токенам; кириллицу не трогает (нет «а»→«ф»).
6. Несколько слов: выделить → `Ctrl+Shift+S` → Заменить нужное; «Оставить» запоминает слова с plaque.

## Команды / логи

- `spell_ignore_add` `{ word }`
- `[mote] spell_ignore add «хмм»`, `layout skip caps «PDF»`, `layout skip ignore …`

## Файлы

`spell_ignore.rs`, `spellcheck.rs`, `layout_fix.rs`, `SpeechBubble.tsx`, `App.tsx`, `permissions/app.toml`
