---
tags: [design, phase-1, spell]
status: active
updated: 2026-10-12
---

# Design: EN словарь для авто-раскладки (режим A)

## Цель
Латиница: если слово есть в **en_US** → не трогать. Если нет в EN, а после EN→RU есть в **ru** (или fuzzy) → сразу заменить текст.

## Данные
- `resources/dictionaries/en_US/en_US.{aff,dic}` (LibreOffice) + копия в `public/dictionaries/en_US/`
- `tauri.conf.json` resources

## Поток (layout_auto)
1. Последнее латинское слово ≥4 + boundary.
2. `english_word_known(word)` → skip (`layout_auto skip en`).
3. Иначе RU exact → replace.
4. Слова из en_US: в чисто латинском поле — skip; в смешанном RU — skip только если ≥3 букв (`bot`/`hello`). Короткие (`ye`→`ну`, `jr`→`ок`) конвертируются.
5. Если в поле уже есть **хотя бы одна** кириллица → остальную латиницу force EN→RU (после `?` тоже: `rfr`→`как`).
6. Иначе (чисто латиница): RU exact / fuzzy.
7. В watcher layout_auto идёт **до** spell_offer и **не ждёт** закрытия plaque («хмм» больше не блокирует строку).
8. Токены включают клавиши-буквы `,.[];'` (`будет` / `хлеб`); одиночная `f`→`а`.
9. ≥2 Latin-токена в фразе → force как mixed (даже без кириллицы).
10. Результат ≤2 букв (`jr`→`ок`) — без Hunspell fuzzy (иначе «гл»/«об»).
11. Голые `.` / `,` (без ASCII-букв в токене) не конвертировать — это пунктуация, не «ю»/«б».
12. URL / git remote: целое поле с `://` / `git@` / `.git` — skip; иначе span `https://…` — не трогать path-токены (`alekseichmsk`, `mote.git`).

## Логи
`hunspell en ready`, `layout_auto skip en «hello»`, `layout_auto ok …`
