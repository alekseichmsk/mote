---
tags: [design, phase-1, spell]
status: active
updated: 2026-10-12
---

# Design: авто-раскладка (режим A)

## Цель
Без выделения и кнопок: после пробела/Enter последнее латинское слово → EN→RU в поле, если Hunspell узнаёт результат.

## Prefs
`%APPDATA%\Mote\prefs.json` → `"layoutAuto": false` (дефолт **выкл**).
Настройки → Основное → «Авто-раскладка».

## Поток (live watcher)
1. `layoutAuto == true`.
2. В delta после boundary (` `, `\n`, `\t`, `.`, `,`, `!`, `?`, …) взять **одно** предыдущее слово.
3. Слово: только `[A-Za-z]`, длина ≥4; граница = пробел/`/`/`?`/пунктуация (`/` = русская точка на EN).
4. Если слово в **en_US** → skip. Иначе EN→RU + RU exact/fuzzy (`ghtdtn`→`привет`). См. [[layout-en-dict]].
5. Успех → polish spell (`spell_offer`: common_fix + движок) → `spell_replace` + раскладка **ru-RU**; **без** plaque.
   Пример: `ghbdtn rfr lbkf` → `превет как дила` → `привет как дела`.
6. После write: `spell_baseline = ""` (иначе Cursor/Chrome diverge → пустой delta навсегда).
7. Idle после границы слова ~220 мс (spell по-прежнему 520 мс); кулдаун между конвертами ~280 мс; тот же токен не чаще раза в 3 с.

## Не трогать
Кнопка «Раскладка», Ctrl+Shift+S, Typograf, LLM. Обычный Cyrillic spell как сейчас.

## Логи
`[mote] layout_auto ok «ghbdtn»→«привет»`, `layout_auto skip …`, `layout_auto off`.

## Риски
Ложные срабатывания на EN-словах; тумблер обязателен. Имена/бренды — skip если не в словаре.
