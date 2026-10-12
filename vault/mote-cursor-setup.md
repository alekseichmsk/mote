---
tags: [cursor, setup]
status: active
updated: 2026-10-10
---

# Mote — настройка Cursor: MCP, скиллы, субагенты, правила

**Применено в репо (2026-10-10):** `ROADMAP.md`, `AGENTS.md`, `.cursor/rules`, `.cursor/skills/mote-*`, `.cursor/agents`, `.cursor/mcp.json`, `tasks/phase-1.md`.  
Канон roadmap: корневой `ROADMAP.md`. Зеркало: [[mote-roadmap]].

Файл для агента Cursor: **создай все файлы ниже в репозитории как есть**. После этого вручную ничего копировать не нужно — агенты сами подхватывают правила, скиллы и субагентов.

Структура:
```
.cursor/
  mcp.json
  rules/
    00-project.mdc
    10-quality.mdc
    20-rust.mdc
    30-ui.mdc
  skills/
    mote-feature/SKILL.md
    mote-debug/SKILL.md
    mote-win32/SKILL.md
    mote-ui/SKILL.md
    mote-smoke-test/SKILL.md
  agents/
    planner.md
    rust-win32.md
    ui.md
    reviewer.md
    qa.md
AGENTS.md
ROADMAP.md
vault/
tasks/
```

---

## 1. MCP-серверы — `.cursor/mcp.json`

Держать `mote-vault` (filesystem на `vault/`) + до 3–4 внешних:

```json
{
  "mcpServers": {
    "mote-vault": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "c:\\project-cursor-2\\newfolder\\vault"]
    },
    "context7":   { "command": "npx", "args": ["-y", "@upstash/context7-mcp"] },
    "thinking":   { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-sequential-thinking"] },
    "playwright": { "command": "npx", "args": ["-y", "@playwright/mcp@latest"] }
  }
}
```

| Сервер | Зачем |
|---|---|
| **mote-vault** | Чтение/запись Obsidian-заметок |
| **context7** | Свежая документация библиотек |
| **thinking** | Пошаговое рассуждение |
| **playwright** | UI smoke в браузерном dev |

Не больше 4–6 серверов одновременно.

---

## 2–5. Шаблоны

Рабочие копии уже в `.cursor/` и корне. Пересоздавать отсюда только если удалили файлы:

- Правила → `.cursor/rules/*.mdc` (project / quality / rust / ui)
- Скиллы → `.cursor/skills/mote-{feature,debug,win32,ui,smoke-test}/SKILL.md`
- Субагенты → `.cursor/agents/{planner,rust-win32,ui,reviewer,qa}.md`
- `AGENTS.md` + `ROADMAP.md` в корне

Дополнительно в репо уже есть: `docs-obsidian`, `bugfix`, `mote-product`, `security`.

---

## 6. Как этим пользоваться

1. Новая фаза: «Используй субагента planner для фазы 2».
2. Задача: «Сделай задачу 1.1.1 по скиллу mote-feature».
3. Баг: «Используй mote-debug: …».
4. Модель: сильная — для планов и Rust/Win32; быстрая — для UI-мелочей.
5. Почаще коммить.

См. [[00-MOC]], [[mote-roadmap]], [[product/roadmap]].
