---
tags: [bug, phase-1]
status: active
updated: 2026-10-10
---

# Spell plaque висит после отправки текста

## Symptom
После Enter/отправки сообщения plaque «опечатка → замена» остаётся над головой (до ~14 с timeout).

## Root cause
`spell_plaque_active` сбрасывался при смене FG и по таймеру 14 с. После send поле часто становится пустым или UIA miss — ветка clear не срабатывала; выход из compose снимал только очки.

## Fix
- Edge Enter → `spell offer clear (enter)`.
- `text_changed` + пустое поле → `spell offer clear (field empty)`.

## Verification
Набрать с опечаткой → plaque → Enter/отправка → plaque сразу пропадает. Лог: `[mote] spell offer clear (enter)` и/или `(field empty)`.
