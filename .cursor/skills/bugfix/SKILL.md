---
name: bugfix
description: Reproduce, diagnose, and minimally fix bugs in Mote; record outcomes in vault/bugs/.
---

# Bugfix

Use when builds fail, the overlay misbehaves, chat breaks, or the user reports a defect.

## Workflow

1. **Reproduce** — exact steps, OS, `npm run tauri dev` vs production.
2. **Isolate** — frontend (`src/`) vs Rust shell (`src-tauri/`) vs config.
3. **Fix minimally** — smallest change that resolves the issue; avoid drive-by refactors.
4. **Verify** — rebuild/typecheck; smoke the overlay path that failed.
5. **Document** — create or update `vault/bugs/YYYY-MM-DD-short-slug.md`.

## Bug note template

```markdown
---
tags: [bug]
status: fixed
updated: YYYY-MM-DD
---

# Title

## Symptom
## Root cause
## Fix
## Verification
```

## Guardrails

- Do not weaken security capabilities to "make it work" without documenting in `vault/security/`.
- Do not disable transparency/always-on-top unless the bug is specifically about window chrome.
- Prefer logging/diagnostics over silent catches when debugging.
