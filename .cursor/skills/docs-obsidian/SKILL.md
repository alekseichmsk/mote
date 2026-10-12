---
name: docs-obsidian
description: Maintain Obsidian-style documentation in vault/ with wikilinks, frontmatter, and MOC updates whenever product or code changes.
---

# Docs Obsidian

Use this skill whenever code, architecture, product decisions, or bugs change in the Mote project.

## Vault rules

- All documentation lives under `vault/`.
- Notes are atomic, short, and linked with `[[wikilinks]]`.
- Every note starts with YAML frontmatter:

```yaml
---
tags: [topic]
status: active
updated: YYYY-MM-DD
---
```

- Keep `vault/00-MOC.md` as the map of content; add links when creating new notes.
- Prefer updating an existing note over creating duplicates.
- Never put secrets, API keys, or `.env` contents into the vault.

## When to write

| Change | Note location |
|---|---|
| Product intent / MVP scope | `vault/product/` |
| Window/overlay/tray architecture | `vault/architecture/` |
| Bug fixed | `vault/bugs/YYYY-MM-DD-short-slug.md` |
| Security decision | `vault/security/` |
| ADR / trade-off | `vault/decisions/` |
| Pet memory / personality facts | `vault/memory/` |

## Style

- Russian or English is fine; prefer Russian for product notes in this repo.
- One idea per note.
- End actionable notes with a short "Next" section if work remains.
