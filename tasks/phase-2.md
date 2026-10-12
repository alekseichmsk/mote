---
tags: [tasks, phase-2]
status: active
updated: 2026-10-11
---

# Фаза 2 — LLM sidecar (первый срез)

Источник: `ROADMAP.md` §Фаза 2. Design: `vault/design/llm-improve-text.md`.

## Срез «Улучшить текст»

| ID | Задача | Статус |
|---|---|---|
| 2.1.1 | Design-doc + этот файл | done |
| 2.1.2 | sidecar + OpenAI client | done |
| 2.1.3 | download GGUF + progress | done |
| 2.1.4 | UI кнопка + selection | done |
| 2.1.5 | Settings статус/скачать | done |
| 2.1.6 | runtime в AppData\Mote\bin (без ручного llama-server) | done |
| 2.1.7 | CUDA GPU runtime + настройка GPU/CPU | done |

**Готово, когда:** нет модели/движка → Улучшить или «Скачать недостающее» → скачивание → sidecar → исправленный текст на plaque → Заменить; словарь без LLM по-прежнему мгновенный; без `MOTE_LLAMA_SERVER`; в Настройках можно выбрать GPU (CUDA) или CPU.

## Позже
- game_mode
- чат через sidecar
- Ollama / облако
- авто-LLM на паузе печати
- Qwen 7B опция
