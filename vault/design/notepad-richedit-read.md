---
tags: [design, phase-1]
status: done
updated: 2026-10-10
---

# 1.1.2a — Win11 Notepad RichEditD2DPT read

## Цель
Читать текст из Блокнота Win11 (`RichEditD2DPT` / Document без ValuePattern) через TextPattern **только** у focused fill-control, без дампа UI.

## Файлы
- `src-tauri/src/field_text.rs` — `is_richedit_editor`, `is_fill_field`, `looks_like_input_rect`, `read_input_value`

## Поток
watcher poll → `read_focused_field_detailed` → resolve fill (Document + RichEdit) → TextPattern document range → `[mote] field read type=Document/RichEditD2DPT len=…`

## Решение
- Класс: UIA `get_classname` или Win32 `GetClassNameW` на hwnd (`RichEditD2DPT` / `RichEdit`)
- Rect: для RichEdit `h≤1600`, иначе chat `h≤480` (веб-Document без класса отсекается)
- Бюджет текста RichEdit: 8k chars / 400 lines (не chat 1200/40)

## Критерий
Notepad: «превет» → `field read` + `spell offer`
