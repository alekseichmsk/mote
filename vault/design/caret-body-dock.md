---
tags: [design, presence]
status: active
updated: 2026-10-12
---

# Design: тело у caret при печати (A)

## Цель
Пока пользователь печатает в поле, Mote **подъезжает** рядом с caret/полем (не только глаза), чтобы plaque был под рукой.

## Поведение
- `composing` + `locked_anchor` → `dock_beside_input`: зона caret+pad≈96, gap 140–280px, сначала **над** полем, **не** поверх текста.
- Глаза / mode `type` без изменений.
- Shoo / `mark_user_placed` / drag по-прежнему важнее авто-дока.

## Файлы
`src-tauri/src/watcher.rs` — ветка composing (убрать `target = None`).

## Логи
Опционально throttle: `[mote] caret dock x= y=` при смене слота.
