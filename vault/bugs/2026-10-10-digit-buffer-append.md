---
tags: [bug, phase-1]
status: open
updated: 2026-10-10
---

# Digit plaque: буфер склеивается / Ctrl+Backspace

## Symptom
1. В поле `23`, plaque `1223`.
2. Ctrl+Backspace стирает всё в поле, у Mote падает одна цифра.
3. Потом текст → иногда bubble «Ничего не нашлось.» (⏱ факт по остатку числа).

## Root cause
`DigitTracker` с клавиатуры: Backspace = `pop` одного символа; Ctrl игнорировался.

## Fix
1. Ctrl+Backspace / Ctrl+Delete → `clear` всего буфера.
2. Sync trailing digits; пустое поле сразу после erase; текст без цифр → clear.
3. Ошибка факта: «Факт об этом числе не нашёлся.» (не wiki-фраза).

## Verification
`123` → Ctrl+Backspace → plaque исчез; набор букв без «Ничего не нашлось.»
