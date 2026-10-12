# Mote

Десктоп-питомец для Windows. Tauri 2 + Rust (`src-tauri`) + React/TS (`src`).
План и архитектура: [[ROADMAP.md|ROADMAP.md]]. Решения и баги: `vault/`. Задачи: `tasks/`.
Cursor-настройка: `vault/mote-cursor-setup.md`.

## Команды

- dev: `npm run tauri dev`
- unit: `npm test`
- API smoke: `npm run qa`
- проверки: `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `npx tsc --noEmit`
- release build (с updater-артефактами): задать `TAURI_SIGNING_PRIVATE_KEY` (= путь к `%USERPROFILE%\.tauri\mote.key`), затем `npm run tauri build`. Updater: https://github.com/alekseichmsk/mote — см. `vault/design/auto-update-release.md`.

## Работающие фичи (не ломать)

- погода (`погода`, город в Настройках)
- Википедия: `? …` / `вики …` → превью + стрелка «перейти»; пусто → «Ничего не нашлось.»
- команды: `столица` / `книга` / `космос` / `цитата` / `факт N` / `факт дня` / `открой`
- музыка: ПКМ под Mote → prev / play / next / громкость (шкала сбоку)
- факты дня ~после 10:00, idle-цитаты/wiki
- анимация очков при печати / caret dock (тело рядом с полем)
- spellcheck в чужих окнах (live = delta; Ctrl+Shift+S; plaque Заменить / Скопировать / Улучшить / Типограф / Раскладка / Оставить→ignore; опц. авто-раскладка: не EN/CAPS → RU после пробела)
- числа → текст (plaque + факт о числе)
- Настройки: вкладки Основное / Команды

## Как работаем

- Для новой фичи — скилл `mote-feature`. Для бага — `mote-debug`.
- Сначала план, жди «ок», потом код. Маленькие диффы, одна задача = одна ветка.
- После кода — ревью субагентом `reviewer`, тест-чеклист субагентом `qa`.
- Документацию в `vault/` обновляй по скиллу `docs-obsidian`.
