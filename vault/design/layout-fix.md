---
tags: [design, phase-1, spell]
status: active
updated: 2026-10-12
---

# Design: раскладка (QWERTY ↔ ЙЦУКЕН)

## Цель
Исправить текст, набранный в неправильной раскладке (`ghbdtn` → `привет` и наоборот). Оффлайн, без новых зависимостей.

## UX
1. **Авто:** `Ctrl+Shift+S` — если словарь не нашёл опечаток, пробуем EN→RU с проверкой Hunspell; при успехе plaque «раскладка → …» и запись как у spell.
2. **Кнопка «Раскладка»** на spell plaque — явный flip по доминирующему алфавиту (EN↔RU), сразу `spell_replace`.

## Поток
- Map в `src-tauri/src/layout_fix.rs`.
- Скоринг EN→RU: доля кириллических слов, известных Hunspell / `common_fix`.
- RU→EN: только по кнопке (нет EN-словаря).
- Skip: URL (`://`, `www.`), email (`@`), пусто / >2000 символов.

## Invoke
- `layout_fix { text, force }` → `Option<String>` (`force` = кнопка без жёсткого скоринга).
- Авто через существующий `check_selection` → `mote://spell-offer`.

## Логи `[mote]`
`layout_fix ok dir=en2ru|ru2en chars=`, `layout_fix skip`, `layout_fix noop`.

## Риски
Ложные срабатывания на латинице (`hello`→`руддщ`) — авто только при хорошем score; кнопка — ответственность пользователя (на plaque с кириллической опечаткой RU→EN испортит текст). Скоринг авто = Hunspell `check`, не Windows Spell.
