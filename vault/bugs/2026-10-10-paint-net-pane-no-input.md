---
tags: [bug, phase-1]
status: fixed
updated: 2026-10-10
---

# Paint.NET Text tool: spell без UIA

## Symptom
UIA: `Pane/WindowsForms10… fill=none`, `win32Edit=none`. Цифры видит (keyboard DigitTracker), текст/spell — нет. Также: случайные клавиши инструментов → очки и цифры над головой без логов canvas.

## Root cause
Text tool на холсте не отдаёт Edit/TextPattern. Глобальный `DigitTracker` + `typing_keys_active` срабатывали на шорткаты, пока canvas active.

## Fix
`canvas_typing.rs`: буфер **всей строки** с клика → `spell_offer` по всей фразе (несколько опечаток сразу) → Replace = Backspace(line) + SendInput(correction). **ARMED** после клика; **COMPOSING** = armed + (line/offer). Цифры — trailing run из line. Watcher не поллит DigitTracker/UIA на canvas.

## Verification
1. Без клика: нет очков/plaque.
2. Text + клик → `превет как дила` → plaque `превет, дила → …` (не забывает первое слово) → Заменить правит всю фразу.
3. `42` → digit plaque → Заменить.