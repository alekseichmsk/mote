---
tags: [tasks, phase-1]
status: active
updated: 2026-10-11
---

# Фаза 1 — Стабилизация spellcheck и числа→текст

Источник: `ROADMAP.md` §«Фаза 1». Баг Replace/Copy: `vault/bugs/2026-10-10-spell-replace-copy.md` (`status: fixed`, smoke 2026-10-10). Live vs on-demand: `vault/decisions/0002-spell-live-vs-ondemand.md`.

**Цель фазы:** в Notepad (Win11), Cursor, Chrome и Word plaque только у поля ввода; Replace/Copy работают; в логах `spell write ok via …`; числа с валютой/датами/% → пропись. Live-spell = delta после baseline; весь файл — on-demand (выделение). Typograf: `vault/design/typograf.md` (**done**). Раскладка: `vault/design/layout-fix.md` (**done**). Авто-раскладка A: `vault/design/layout-fix-auto.md` (**done** 2026-10-12).

**Не ломать** (регресс после каждой задачи): погода, вики `?`/`вики`, команды, музыка ПКМ, факты дня, очки при печати, базовый spell/digits plaque, Настройки.

---

## Реальность кода (срез 2026-10-10)

| Область | Уже есть | Пробел |
|---|---|---|
| Replace/Copy UI | `pointerdown` → `spell_replace` / `digit_copy`; Win32 clipboard | — (баг закрыт) |
| Фокус перед записью | `AttachThreadInput` + `SetForegroundWindow` + ~50 мс (`digit_actions.rs`) | Soft-success без verify поля |
| Цепочка записи | RichEdit → Ctrl+V first; иначе UIA → EM_* → WM_PASTE → Ctrl+V | Smoke verify Notepad Replace → `spell write ok via Ctrl+V (RichEdit)` |
| Чтение поля | ValuePattern → TextPattern; RichEditD2DPT / Document; лог `field read` | Smoke Notepad «превет» → plaque |
| Electron | HWND 0 допустим; Ctrl+V fallback; комментарий про lazy tree | Нет явной задержки/ретрая дерева; матрица Cursor/Chrome не закрыта |
| Числа→текст | `numberWords.ts`: только `\d+` → целые слова | Нет валюты, копеек, дат, %, падежей единиц |

---

## Порядок работ (стартовать сверху)

1. **1.1.4** — диагностика — **done** (smoke в Notepad).
2. **1.1.2a** — Win11 Notepad read — **done** (код: RichEditD2DPT / Document + TextPattern; ждать smoke «превет»).
3. **1.1.2b** — Win11 Notepad write — **done** (код: RichEdit → Ctrl+V first; smoke Replace).
4. **1.1.2c** — Electron/Chromium — **partial** (Cursor ok + retry; Chrome smoke → 1.1.5).
5. **1.1.3** — verify-only фокус-цепочки (код уже есть; править только если матрица падает).
6. **1.1.5** — матрица smoke Definition of Done фазы ← следующее для закрытия 1.1.
7. **1.2.1 → 1.2.2 → 1.2.3** — валюта → даты → проценты (после стабильного spell write).

**1.1.1** — не открывать заново; только при регрессии.

---

## 1.1 Spellcheck в чужих окнах

### 1.1.1 — Smoke Replace/Copy (закрыт)

- **Статус:** `done`
- **Файлы:** не трогать, кроме регресса. Баг: `vault/bugs/2026-10-10-spell-replace-copy.md`
- **Критерий:** пользовательский smoke 2026-10-10: Replace/Copy + клик при «очках» ок; баг `fixed`.
- **Проверка:** не требуется, пока нет новой жалобы. При сомнении — один прогон Notepad: опечатка → Заменить / Скопировать; логи `[mote] spell_replace invoked`, `[mote] clipboard set`, `[mote] spell write ok via …`.
- **Зависимости:** нет
- **Риски:** не переоткрывать как open work; остаточные дыры → 1.1.2 / 1.1.3

---

### 1.1.4 — Диагностика field read + ошибки invoke

- **Статус:** `done` (smoke: `Edit len=…`; доработан подъём hwnd с родителя при `0x0`)
- **Файлы likely:** `src-tauri/src/field_text.rs`, `src-tauri/src/watcher.rs` (место poll), при необходимости `src/App.tsx` / `src/lib/spellcheck.ts` (ошибки invoke в консоль)
- **Критерий:**
  - При успешном чтении поля: лог вида `[mote] field read type=… len=… hwnd=…` (control type + длина текста).
  - При провале чтения в watch-окне — короткий лог причины (focus не input / пусто / budget), без спама каждый тик.
  - Ошибки `spell_replace` / `digit_copy` / `digit_replace` видны в webview console (`console.warn` с префиксом `[mote]`).
- **Проверка:**
  1. `npm run tauri dev`, печатать в Notepad.
  2. В stderr: `[mote] field read …` с ненулевой `len` и типом (Document/Edit/…).
  3. Сломать invoke (временно неверный ACL / оффлайн-сборка без команды) → в DevTools console есть `[mote] … failed`.
- **Зависимости:** нет (делать первой)
- **Риски:** шум логов при poll ~320 мс — логировать только смену текста/типа или throttle; не логировать весь текст поля (PII)

---

### 1.1.2a — Win11 Notepad: чтение RichEditD2DPT

- **Статус:** `done` (код 2026-10-10; smoke: Notepad «превет» → `field read type=Document/RichEdit…` + plaque)
- **Файлы:** `src-tauri/src/field_text.rs`, `vault/design/notepad-richedit-read.md`
- **Сделано:**
  - `is_richedit_editor` (UIA classname + Win32 `GetClassNameW`)
  - Document + RichEdit принимается без жёсткого `h≤480`; TextPattern на focused fill; бюджет RichEdit до 8k
  - лог `type=Document/RichEditD2DPT` (класс в control_type)
- **Критерий:**
  - Focused Document без ValuePattern (класс окна/контрола `RichEditD2DPT` или эквивалент): текст берётся **только** через TextPattern focused-элемента, не дамп страницы.
  - Plaque появляется на опечатке в Notepad Win11; лог `field read` показывает разумную `len` (не весь UI).
  - Не читать Pane/Window/Text как поле.
- **Проверка:**
  1. Блокнот Win11, строка с «превет».
  2. После паузы печати (~2.5 с watch): plaque с исправлением.
  3. Логи: `[mote] field read type=Document …` (или Edit), `len` ≈ длины строки; нет гигантского текста.
- **Зависимости:** желательно после **1.1.4**
- **Риски:** `looks_like_input_rect` отсечёт огромное окно Notepad — подобрать порог; не сломать Chrome chat Document

---

### 1.1.2b — Win11 Notepad: запись Replace

- **Статус:** `done` (код 2026-10-10; smoke: Replace → `spell write ok via Ctrl+V (RichEdit)`)
- **Файлы:** `src-tauri/src/digit_actions.rs` (`replace_spell`, `is_richedit_hwnd`)
- **Сделано:**
  - `RichEdit` / `RichEditD2DPT`: сразу clipboard + Ctrl+A/V, без `EM_REPLACESEL`
  - verify через `read_field_hwnd` / focused TextPattern
  - классический Edit по-прежнему EM_* → WM_PASTE → Ctrl+V
- **Критерий:**
  - Заменить в Notepad Win11 меняет текст в поле (не только clipboard).
  - Ветки: не полагаться на `EM_REPLACESEL` для RichEditD2DPT; приоритет UIA (если есть) → фокус + Ctrl+A/V / `WM_PASTE` с verify через `read_field_hwnd`.
  - Лог успеха: `[mote] spell write ok via …` (UIA / Ctrl+V / WM_PASTE), не только `field not verified; clipboard ready`.
- **Проверка:**
  1. Опечатка → Заменить → текст в Notepad исправлен.
  2. Скопировать → буфер содержит исправление (`[mote] clipboard set`).
  3. Регресс: классический Edit (если доступен) и Wordpad/Word не ломаются.
- **Зависимости:** **1.1.2a** (нужно стабильное чтение для verify)
- **Риски:** Ctrl+A затрёт весь документ — ок для Notepad single-field; в Word опаснее → писать по сохранённому HWND / не select-all если UIA set value сработал

---

### 1.1.2c — Electron / Chromium (Cursor, Chrome)

- **Статус:** `partial` — Cursor зелёный (smoke 2026-10-10, без spike); Chrome smoke на `vault/qa/chrome-spell-smoke.html`
- **Файлы:** `src-tauri/src/field_text.rs` (retry ×2 ×45 мс на `not input` / `unreadable` / `no focus`), `digit_actions` Ctrl+V path, `vault/qa/chrome-spell-smoke.html`
- **Сделано:**
  - Cursor: `превет` → Replace → `[mote] spell write ok via Ctrl+V` (hwnd Win32 fallback)
  - Lazy a11y: короткий retry в `read_focused_field_detailed` (не в UI-потоке React)
  - Smoke-страница: input + contenteditable
- **Критерий:**
  - В Cursor (поле чата/editor contenteditable) и Chrome (input / contenteditable) plaque только у focused input; Replace/Copy работают.
  - Учитывать lazy accessibility tree: короткий retry/delay перед read после typing/click (без глобального sleep в UI-потоке дольше необходимого).
  - HWND 0 + `top_hwnd` — путь Ctrl+V остаётся рабочим.
- **Проверка:**
  1. Cursor: опечатка в input → plaque → Заменить / Скопировать.
  2. Chrome: открыть `vault/qa/chrome-spell-smoke.html` — `<input>` и contenteditable — то же.
  3. Логи: `[mote] spell_replace …`, `[mote] spell write ok via Ctrl+V` или UIA; `[mote] clipboard set` на Copy.
- **Зависимости:** **1.1.4**; желательно после **1.1.2b** (общий write-path)
- **Риски:** ложные срабатывания на всей странице Document; не сломать музыку/очки; Electron hwnd=0 → только top-level FG

---

### 1.1.3 — Фокус: AttachThreadInput + SetForegroundWindow (verify / harden)

- **Статус:** `done` (код на месте) → при падении матрицы перевести в `open` точечно
- **Файлы likely:** `src-tauri/src/digit_actions.rs` (`focus_hwnd`, `activate_target`, `paste_replaced`)
- **Уже в коде:** AttachThreadInput (cur/fg/target) → SetForegroundWindow → SetFocus → detach → sleep 50 мс; retry focus ×3; цепочка UIA → EM → WM_PASTE → Ctrl+V.
- **Критерий готовности (verify):**
  - Клик по plaque не оставляет запись в Mote; целевое окно снова foreground до SendInput.
  - После Replace лог `spell write ok via …`, не стабильный soft-success.
- **Проверка:** клик Заменить из overlay при открытом Notepad/Cursor; смотреть `[mote] spell focus retry` (редко) и итоговый `spell write ok via …`.
- **Зависимости:** проверять вместе с **1.1.2b/c**
- **Риски:** лишний рефактор фокуса без падающего кейса — не делать; soft-success маскирует провал → ужесточить только если матрица красная

---

### 1.1.5 — Матрица smoke Definition of Done (фаза 1.1)

- **Статус:** `open`
- **Файлы:** документация результата в `vault/` (по скиллу docs-obsidian) при закрытии; код только если матрица нашла баг
- **Критерий:** таблица ниже зелёная на одной машине Win11.

| Приложение | Plaque только у input | Replace | Copy | Лог `spell write ok via …` |
|---|---|---|---|---|
| Notepad Win11 | | | | |
| Cursor | ✅ (Edit/aislash) | ✅ | | ✅ Ctrl+V (2026-10-10) |
| Chrome | | | | |
| Word (или WordPad) | ✅ Document/_WwG | ✅ | | ✅ Ctrl+V (2026-10-10) |

- **Проверка:** чеклист субагента `qa` / скилл `mote-smoke-test`; регресс AGENTS.md фич — короткий прогон.
- **Зависимости:** **1.1.2a–c**, **1.1.4**; **1.1.3** verify
- **Риски:** Word UIA отличается — зафиксировать фактический путь записи в vault

---

## 1.2 Числа → текст

Базовый путь работает: watcher digits → plaque → `numberToRussianWords` / `digitWordsOffer` (только целые `\d+`). Расширения — маленькими диффами в `src/lib/numberWords.ts` (+ тесты), wiring в `App.tsx` / `SpeechBubble.tsx` без ломки текущего offer.

### 1.2.1 — Суммы с валютой и падежи

- **Статус:** `open`
- **Файлы likely:** `src/lib/numberWords.ts`, новый `src/lib/numberWords.test.ts` (или рядом), `src/App.tsx` (сборка `words` из digit-offer), при необходимости watcher (распознавание «грязного» числа с пробелами/запятой)
- **Критерий:**
  - Пример ROADMAP: `1 250,50 руб.` → `одна тысяча двести пятьдесят рублей 50 копеек` (допустимы мелкие варианты формулировки, смысл и падежи верны).
  - Поддержка маркеров: `руб` / `руб.` / `₽` / `рублей`; целая часть + копейки (`,` или `.`).
  - Падежи: рубль/рубля/рублей, копейка/копейки/копеек; тысячи женский род («одна тысяча»).
  - Старый путь «42» → «сорок два» не ломается.
- **Проверка:**
  1. Unit: `npm test` на фикстурах сумм.
  2. Ручной: ввести/вставить сумму в чужом поле → plaque → Заменить/Скопировать.
  3. Логи copy/replace как у digits: `[mote] clipboard set`, `[mote] digit_replace invoked` / успех замены.
- **Зависимости:** стабильный digit replace (базовый уже есть); не блокируется 1.1, но лучше после **1.1.5** если пишешь в то же поле
- **Риски:** watcher сейчас копит «чистые» цифры — суммы с пробелом/`руб` могут не попасть в digit tracker → парсить из `field_text` или расширить tracker; не трогать конвертер валют (в «Не делаем» ROADMAP)

---

### 1.2.2 — Даты прописью

- **Статус:** `open`
- **Файлы likely:** `src/lib/numberWords.ts` (+ тесты), wiring offer
- **Критерий:**
  - Распознавать простые RU-даты: `10.10.2026`, `10/10/2026` (и при необходимости `2026-10-10`) → читаемая пропись (день + месяц + год), без ломания обычных чисел.
  - Ложных срабатываний на IP/`1.2` минимум (эвристика: диапазоны день/месяц/год).
- **Проверка:** unit-фикстуры + ручной ввод даты в Notepad → plaque → Copy.
- **Зависимости:** **1.2.1** (общий парсер «числовых фраз» в numberWords)
- **Риски:** конфликт с десятичными суммами (`1.50`) — порядок правил: валюта → дата → голое число

---

### 1.2.3 — Проценты

- **Статус:** `open`
- **Файлы likely:** `src/lib/numberWords.ts` (+ тесты), wiring; watcher: Shift+5 = `%` сейчас рвёт digit run — учесть ввод `12%` / `12 %`
- **Критерий:**
  - `12%` / `12 %` → `двенадцать процентов` (падеж процент/процента/процентов).
  - Не ломать ввод `%` как символа без числа.
- **Проверка:** unit + ручной ввод в поле → plaque; регресс «голых» цифр и валюты.
- **Зависимости:** **1.2.1**
- **Риски:** digit tracker обрывается на modifiers (`watcher.rs` ~387) — offer может идти только из field snapshot; согласовать с spell watch window

---

## Definition of Done фазы 1

- [ ] Матрица **1.1.5** зелёная
- [ ] В логах стабильно `[mote] spell write ok via …` на целевых приложениях
- [ ] Примеры валюты / даты / % из ROADMAP проходят (**1.2.1–1.2.3**)
- [ ] Баг spell replace/copy остаётся `fixed`; регресс AGENTS.md пройден
- [ ] ROADMAP чекбоксы фазы 1 можно отметить после закрытия задач

---

## Сводка статусов

| ID | Заголовок | Статус |
|---|---|---|
| 1.1.1 | Smoke Replace/Copy | **done** |
| 1.1.4 | Диагностика field read / invoke | **done** |
| 1.1.2a | Notepad RichEditD2DPT read | **open** |
| 1.1.2b | Notepad write | **open** |
| 1.1.2c | Electron/Chromium | **partial** — Cursor Edit: Replace via Ctrl+V ok (2026-10-10) |
| 1.1.3 | Фокус AttachThreadInput… | **done** (verify: Ctrl+V path) |
| 1.1.5 | Матрица smoke DoD | **open** |
| 1.2.1 | Валюта + падежи | **open** |
| 1.2.2 | Даты | **open** |
| 1.2.3 | Проценты | **open** |
