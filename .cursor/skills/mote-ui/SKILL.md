---
name: mote-ui
description: Используй при создании или изменении интерфейса: plaque, настройки, редактор скриншотов, анимации персонажа.
---
- Компоненты — через shadcn MCP (`add` нужного компонента), не руками (после миграции на shadcn).
- Анимации — Motion; персонаж — Rive state machine, входы из PetState
  (idle, typing, thinking, listening, speaking, focus, break, sleep, happy, surprised).
- Редактор скриншотов — Konva (Stage/Layer, объекты с трансформером, undo/redo через историю состояний).
- Окна Tauri: прозрачность и window-vibrancy (Mica/Acrylic), always-on-top для питомца.
- Проверяй вёрстку в dev-режиме через playwright MCP скриншотом, если возможно.
- Тёмная и светлая тема, prefers-reduced-motion.
- Медиабар не должен сдвигать питомца вверх; без тяжёлых теней поверх чужих окон.
