//! On-demand spell: selected text via hotkey (Ctrl+Shift+S). No host RMB menu —
//! that conflicted with Cursor/Chrome context menus.

use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

static CACHED_SELECTION: Mutex<Option<crate::field_text::FieldSnapshot>> = Mutex::new(None);

fn take_cached_selection() -> Option<crate::field_text::FieldSnapshot> {
    CACHED_SELECTION.lock().ok().and_then(|mut g| g.take())
}

fn cache_selection(snap: crate::field_text::FieldSnapshot) {
    if let Ok(mut g) = CACHED_SELECTION.lock() {
        *g = Some(snap);
    }
}

/// Run dictionary spell on the current selection and emit a plaque offer.
pub fn check_selection(app: AppHandle) -> Result<(), String> {
    eprintln!("[mote] selection spell: check");
    // Prefer snapshot cached at RMB time — menu click steals focus from the host app.
    let snap = match take_cached_selection() {
        Some(s) => s,
        None => crate::field_text::read_selection()?,
    };
    eprintln!(
        "[mote] selection spell: len={} hwnd={:#x}",
        snap.text.chars().count(),
        snap.hwnd
    );
    match crate::spellcheck::spell_offer(app.clone(), snap.text.clone()) {
        Ok(Some(offer)) => {
            crate::digit_actions::set_selection_replace_mode(true);
            let top = {
                #[cfg(windows)]
                {
                    win_fg()
                }
                #[cfg(not(windows))]
                {
                    0
                }
            };
            crate::digit_actions::set_digit_target_ex(snap.hwnd, top, &offer.replaced);
            eprintln!(
                "[mote] selection spell offer: «{}» → «{}»",
                offer.word,
                offer.suggestions.first().map(|s| s.as_str()).unwrap_or("")
            );
            try_apply_if_field_is_selection(snap.hwnd, top, &snap.text, &offer.replaced);
            let _ = app.emit("mote://spell-offer", offer);
            Ok(())
        }
        Ok(None) => {
            // Latin typed on EN while meaning RU — spell_offer ignores non-Cyrillic.
            if let Some(fixed) = crate::layout_fix::suggest_en_to_ru(&app, &snap.text) {
                crate::digit_actions::set_selection_replace_mode(true);
                let top = {
                    #[cfg(windows)]
                    {
                        win_fg()
                    }
                    #[cfg(not(windows))]
                    {
                        0
                    }
                };
                crate::digit_actions::set_digit_target_ex(snap.hwnd, top, &fixed);
                eprintln!(
                    "[mote] selection layout offer: «{}» → «{}»",
                    snap.text.chars().take(32).collect::<String>(),
                    fixed.chars().take(32).collect::<String>()
                );
                try_apply_if_field_is_selection(snap.hwnd, top, &snap.text, &fixed);
                let offer = crate::spellcheck::SpellOfferPayload {
                    word: "раскладка".into(),
                    suggestions: vec![fixed.clone()],
                    start: 0,
                    end: snap.text.len(),
                    replaced: fixed.clone(),
                    original: snap.text,
                    fixes: vec![],
                };
                let _ = app.emit("mote://spell-offer", offer);
                return Ok(());
            }
            crate::digit_actions::set_selection_replace_mode(false);
            let _ = app.emit(
                "mote://spell-offer",
                crate::spellcheck::SpellOfferPayload::empty(),
            );
            Err("в выделении нет опечаток".into())
        }
        Err(e) => Err(e),
    }
}

/// Improve cached/current selection via local LLM and show as spell plaque.
pub fn improve_selection(app: AppHandle) -> Result<(), String> {
    eprintln!("[mote] selection llm: improve");
    let snap = match take_cached_selection() {
        Some(s) => s,
        None => crate::field_text::read_selection()?,
    };
    let st = crate::llm::status();
    if !st.model_present {
        cache_selection(snap);
        crate::llm::download_model(app)?;
        return Err("DOWNLOADING".into());
    }
    if !st.runtime_present {
        // Keep selection cache for retry after download.
        cache_selection(snap);
        crate::llm::download_runtime(app)?;
        return Err("DOWNLOADING".into());
    }
    // Capture target before long sidecar/inference wait (selection won't survive).
    let top = {
        #[cfg(windows)]
        {
            win_fg()
        }
        #[cfg(not(windows))]
        {
            0
        }
    };
    crate::digit_actions::set_selection_replace_mode(true);
    crate::digit_actions::set_digit_target_ex(snap.hwnd, top, &snap.text);
    let result = crate::llm::improve_text(app.clone(), snap.text.clone())?;
    let changed = result.improved.trim() != snap.text.trim();
    eprintln!(
        "[mote] llm: improve result changed={} chars={}",
        changed,
        result.improved.chars().count()
    );
    if !changed {
        // 3B often echoes a one-word typo. Dictionary still knows «превет» → «привет».
        if let Ok(Some(offer)) = crate::spellcheck::spell_offer(app.clone(), snap.text.clone()) {
            eprintln!(
                "[mote] llm: fallback spell «{}»",
                offer.suggestions.first().map(|s| s.as_str()).unwrap_or("")
            );
            crate::digit_actions::set_digit_target_ex(snap.hwnd, top, &offer.replaced);
            try_apply_if_field_is_selection(snap.hwnd, top, &snap.text, &offer.replaced);
            let _ = app.emit("mote://spell-offer", offer);
            return Ok(());
        }
        let _ = app.emit("mote://selection-spell-status", "уже без правок");
        return Ok(());
    }
    crate::digit_actions::set_digit_target_ex(snap.hwnd, top, &result.improved);
    try_apply_if_field_is_selection(snap.hwnd, top, &snap.text, &result.improved);
    let offer = crate::spellcheck::SpellOfferPayload {
        word: "выделение".into(),
        suggestions: vec![result.improved.clone()],
        start: 0,
        end: snap.text.len(),
        replaced: result.improved.clone(),
        original: snap.text,
        fixes: vec![],
    };
    let _ = app.emit("mote://spell-offer", offer);
    Ok(())
}

/// Selection menu stole focus. If the box still holds only that text, write the fix in.
fn try_apply_if_field_is_selection(hwnd: isize, top: isize, original: &str, corrected: &str) {
    let Some(current) = crate::field_text::read_field_hwnd(hwnd) else {
        eprintln!("[mote] selection apply skip: field unread");
        return;
    };
    if current.trim() != original.trim() || corrected.trim() == original.trim() {
        eprintln!("[mote] selection apply skip: field != selection");
        return;
    }
    crate::digit_actions::set_selection_replace_mode(false);
    crate::digit_actions::set_digit_target_ex(hwnd, top, original);
    match crate::digit_actions::replace_spell(corrected, corrected) {
        Ok(()) => eprintln!("[mote] selection apply ok"),
        Err(e) => eprintln!("[mote] selection apply err: {e}"),
    }
}

pub fn start(_app: AppHandle) {
    // Host RMB menu removed — use Ctrl+Shift+S for selection spell.
}

pub fn stop() {}

#[cfg(windows)]
fn win_fg() -> isize {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe { GetForegroundWindow().0 as isize }
}
