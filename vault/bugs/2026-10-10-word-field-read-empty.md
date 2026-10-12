---
tags: [bug, phase-1]
status: fixed
updated: 2026-10-10
---

# Word: field read miss empty

## Symptom
Microsoft Word, набор текста → `[mote] field read miss: empty`. Plaque нет.

## Root cause (diag 2026-10-10)
```
focus=Document/_WwG … val=false textPat=true winClass=_WwG
| fill=Edit name="Содержимое страницы 1" val=false textPat=true
```
Document `_WwG` не считался fill (высокий rect, не RichEdit). Resolve брал дочерний Edit с пустым Value; TextPattern на Edit был отключён → `empty`.

## Fix
1. `is_word_document` (`_WwG`) + tall surface budget как у RichEdit.
2. Resolve: focused Document Word/RichEdit сразу, не child Edit.
3. `read_input_value`: пустой Value не успех; fallback TextPattern (Word / TipTap).

## Verification
Smoke 2026-10-10: `Document/_WwG` len=1…6 → offer «превет» → Replace → `spell write ok via Ctrl+V`.
