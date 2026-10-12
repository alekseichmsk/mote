---
tags: [bug, phase-1]
status: fixed
updated: 2026-10-10
---

# Spell offer на spike 6→802 chars (Cursor Edit)

## Symptom
В Cursor (`Edit/aislash-editor-input`) при наборе «превет» лог:
`field read … len=6` → `len=802` → `spell offer: «превет, превет» (replaced 802 chars)`, затем снова `len=6`.
Replace при нормальном оффере (7 chars) работает: `spell write ok via Ctrl+V`.

## Root cause
TextPattern document range на Chromium `Edit` иногда отдаёт огромный blob; `replaced` = весь текст поля → ложный plaque.

## Fix
1. `field_text::read_input_value` — для Edit/ComboBox/Spinner только ValuePattern / ValueValue, без TextPattern.
2. `watcher` — игнор spike (`prev<80` и рост ×4 / +100) с логом `field read miss: spike`.

## Verification
Smoke 2026-10-10: `len=1…6`, оффер «превет» (replaced 6), без spike/802; `spell write ok via Ctrl+V`.
