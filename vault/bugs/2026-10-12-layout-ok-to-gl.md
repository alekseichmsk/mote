---
tags: [bug, layout]
status: patch
updated: 2026-10-12
---

# layout_auto: «ок» → «гл»

## Симптом

Авто-раскладка после `jr` (или похожего) писала не «ок», а «гл» / раньше «об».

## Причина

`jr`→`ок` верно, но «ок» нет в Hunspell → `suggest_cyrillic_near` подставлял чужой short-fix. Polish ≤2 уже не трогал; **fuzzy в `resolve_layout_word`** — трогал.

## Фикс

В `resolve_layout_word`: при длине результата ≤2 принимать exact convert, без fuzzy.

## Проверка

EN-раскладка: `jr ` → в поле «ок», лог `[mote] layout_auto short «jr»→«ок»` (не fuzzy).
