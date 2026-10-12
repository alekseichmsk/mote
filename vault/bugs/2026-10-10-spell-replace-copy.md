---
tags: [bug]
status: fixed
updated: 2026-10-10
---

# Spell replace / copy failed

## Symptom
Corrected text did not replace the field and copy left the clipboard empty.

## Root cause
1. Overlay click made Mote foreground; UIA wrote to the focused (wrong) element.
2. Frontend used `navigator.clipboard` first; it fails in the Tauri webview and skipped Win32 `digit_copy`.
3. Plaque buttons used `onMouseDown preventDefault` + `onClick` — in WebView2 the click often never fired, so Rust never ran (no `[mote] clipboard set` logs).

## Fix
- Write/read field by stored HWND via UIA (`write_field_hwnd` / `read_field_hwnd`).
- Stronger focus restore + Ctrl+A/V + `WM_PASTE` fallback; always set Win32 clipboard.
- Copy/replace UI prefers `digit_copy` / `spell_replace` Rust paths.
- Buttons fire on `pointerdown`; ACL `allow-mote-commands` for invoke.

## Verification
User smoke 2026-10-10: working (spell replace/copy + click while glasses).
