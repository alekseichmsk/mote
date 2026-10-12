---
tags: [decision, phase-1, spell]
status: active
updated: 2026-10-11
---

# Spell: live delta vs on-demand selection

## Decision
- **Live** (печать): только append-delta после `spell_baseline`; offer после границы слова (пробел/пункт), не mid-word. Правка внутри baseline → live off, только on-demand.
- **On-demand**: выделение → `Ctrl+Shift+S` или ПКМ → меню «Проверить орфографию» (selection кэшируется до клика); Replace = Ctrl+V поверх выделения.
- **Частые опечатки** (`тоесть`→`то есть`) — таблица `common_fix` в `spellcheck.rs`, приоритет над Windows/Hunspell.
- **Грамматика / запятые / смысл** — фаза LLM «Улучшить текст» ([[../ROADMAP|ROADMAP]]); словарь по-прежнему не понимает контекст.
- **Underline** — позже, только UIA TextPattern ranges; Paint/Photoshop canvas — только plaque.

## Why
Большой Notepad заливал plaque чужими словами; mid-word (`смо→само`) мешал набору. Словарь не чинит смысл (`арфографию→ареографию`).

## Code
- `spellcheck::live_spell_scope` / `apply_scope_to_offer` / debounce helpers
- `watcher` baseline + idle 520 ms
- `selection_spell` + `field_text::read_selection`
