---
tags: [design, ui, settings]
status: active
updated: 2026-10-12
---

# Settings + spell plaque UI

## Цель

Убрать «сырой» вид настроек и plaque; бренд Mote (розовый `#e21f7a` / `#ff5aa8`).

## Plaque

- Группы: Заменить/Копировать · Улучшить/Типограф/Раскладка/Оставить · Закрыть
- Компактная панель, розовый акцент
- Шире bubble, мягкий фон

## Настройки

- Окно frameless (`decorations: false`, `transparent`): своя chrome-полоса, drag, ×
- Скруглённая оболочка + розовая обводка
- Карточки: Питомец / Текст / Модель
- Toggle авто-раскладки, chip GPU/CPU

## Файлы

`SpeechBubble.tsx`, `SettingsApp.tsx`, `styles.css`, `src-tauri/src/lib.rs` (`open_settings`)
