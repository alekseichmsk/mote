//! Spellcheck: Windows Spell Checking API (`ru-RU`) first, Hunspell LibreOffice fallback.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellFix {
    pub word: String,
    pub suggestion: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellOfferPayload {
    /// Misspelled words joined for the plaque, e.g. "превет, дила".
    pub word: String,
    /// First entry = readable fix summary; also used as clipboard fallback.
    pub suggestions: Vec<String>,
    pub start: usize,
    pub end: usize,
    /// Full field text with every typo corrected.
    pub replaced: String,
    /// Raw field text before dictionary fixes — for LLM «Улучшить».
    pub original: String,
    pub fixes: Vec<SpellFix>,
}

impl SpellOfferPayload {
    pub fn empty() -> Self {
        Self {
            word: String::new(),
            suggestions: Vec::new(),
            start: 0,
            end: 0,
            replaced: String::new(),
            original: String::new(),
            fixes: Vec::new(),
        }
    }
}

struct SpellState {
    /// True when Windows `ru-RU`/`ru` spell service is installed.
    #[cfg(windows)]
    win_available: bool,
    hunspell: Option<spellbook::Dictionary>,
    /// Optional en_US for layout_auto (skip real English words).
    hunspell_en: Option<spellbook::Dictionary>,
}

static SPELL: OnceLock<Mutex<Option<SpellState>>> = OnceLock::new();

fn spell_slot() -> &'static Mutex<Option<SpellState>> {
    SPELL.get_or_init(|| Mutex::new(None))
}

// ISpellChecker is apartment-threaded — keep one per OS thread.
#[cfg(windows)]
thread_local! {
    static WIN_CHECKER: std::cell::RefCell<
        Option<windows::Win32::Globalization::ISpellChecker>,
    > = const { std::cell::RefCell::new(None) };
}

fn dict_paths(app: &AppHandle, lang_dir: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(dir) = app.path().resource_dir() {
        paths.push(dir.join("dictionaries").join(lang_dir));
    }

    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let root = PathBuf::from(manifest);
        paths.push(root.join("resources").join("dictionaries").join(lang_dir));
        paths.push(
            root.join("..")
                .join("public")
                .join("dictionaries")
                .join(lang_dir),
        );
    }

    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("resources").join("dictionaries").join(lang_dir));
        paths.push(cwd.join("dictionaries").join(lang_dir));
    }

    paths
}

fn load_hunspell_lang(app: &AppHandle, lang: &str) -> Option<spellbook::Dictionary> {
    let aff_name = format!("{lang}.aff");
    let dic_name = format!("{lang}.dic");
    for dir in dict_paths(app, lang) {
        let aff_path = dir.join(&aff_name);
        let dic_path = dir.join(&dic_name);
        if !aff_path.is_file() || !dic_path.is_file() {
            continue;
        }
        let Ok(aff) = std::fs::read_to_string(&aff_path) else {
            continue;
        };
        let Ok(dic) = std::fs::read_to_string(&dic_path) else {
            continue;
        };
        match spellbook::Dictionary::new(&aff, &dic) {
            Ok(dict) => {
                eprintln!("[mote] hunspell {lang} ready from {}", dir.display());
                return Some(dict);
            }
            Err(e) => {
                eprintln!("[mote] hunspell {lang} load error {}: {e:?}", dir.display());
            }
        }
    }
    None
}

#[cfg(windows)]
fn ensure_com() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    unsafe {
        // S_OK / S_FALSE / RPC_E_CHANGED_MODE — all fine for our use.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

#[cfg(windows)]
fn windows_ru_supported() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Globalization::{ISpellCheckerFactory, SpellCheckerFactory};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    ensure_com();
    unsafe {
        let Ok(factory) = CoCreateInstance::<_, ISpellCheckerFactory>(
            &SpellCheckerFactory,
            None,
            CLSCTX_INPROC_SERVER,
        ) else {
            return false;
        };
        for tag in ["ru-RU", "ru"] {
            let wide: Vec<u16> = tag.encode_utf16().chain(std::iter::once(0)).collect();
            if factory
                .IsSupported(PCWSTR(wide.as_ptr()))
                .map(|b| b.as_bool())
                .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

#[cfg(windows)]
fn create_windows_checker() -> Option<windows::Win32::Globalization::ISpellChecker> {
    use windows::core::PCWSTR;
    use windows::Win32::Globalization::{ISpellCheckerFactory, SpellCheckerFactory};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    ensure_com();
    unsafe {
        let factory = CoCreateInstance::<_, ISpellCheckerFactory>(
            &SpellCheckerFactory,
            None,
            CLSCTX_INPROC_SERVER,
        )
        .ok()?;
        for tag in ["ru-RU", "ru"] {
            let wide: Vec<u16> = tag.encode_utf16().chain(std::iter::once(0)).collect();
            if !factory
                .IsSupported(PCWSTR(wide.as_ptr()))
                .map(|b| b.as_bool())
                .unwrap_or(false)
            {
                continue;
            }
            if let Ok(checker) = factory.CreateSpellChecker(PCWSTR(wide.as_ptr())) {
                eprintln!("[mote] Windows Spell API ready on thread ({tag})");
                return Some(checker);
            }
        }
    }
    None
}

#[cfg(windows)]
fn with_windows_checker<R>(
    f: impl FnOnce(&windows::Win32::Globalization::ISpellChecker) -> R,
) -> Option<R> {
    WIN_CHECKER.with(|cell| {
        if cell.borrow().is_none() {
            *cell.borrow_mut() = create_windows_checker();
        }
        let borrow = cell.borrow();
        Some(f(borrow.as_ref()?))
    })
}

fn ensure_loaded(app: &AppHandle) -> Result<(), String> {
    {
        let guard = spell_slot().lock().map_err(|_| "spell lock".to_string())?;
        if guard.is_some() {
            return Ok(());
        }
    }

    #[cfg(windows)]
    let win_available = {
        let ok = windows_ru_supported();
        if ok {
            eprintln!("[mote] Windows Spell API (ru) available");
        } else {
            eprintln!("[mote] Windows Spell API: ru-RU/ru not installed — Hunspell fallback");
        }
        ok
    };

    let hunspell = load_hunspell_lang(app, "ru_RU");
    let hunspell_en = load_hunspell_lang(app, "en_US");
    if hunspell_en.is_none() {
        eprintln!("[mote] hunspell en_US missing — layout_auto EN skip disabled");
    }

    #[cfg(windows)]
    let has_engine = win_available || hunspell.is_some();
    #[cfg(not(windows))]
    let has_engine = hunspell.is_some();

    if !has_engine {
        return Err("нет ни Windows Spell API (ru-RU), ни словаря Hunspell".into());
    }

    let mut guard = spell_slot().lock().map_err(|_| "spell lock".to_string())?;
    if guard.is_none() {
        *guard = Some(SpellState {
            #[cfg(windows)]
            win_available,
            hunspell,
            hunspell_en,
        });
    }
    Ok(())
}

/// Warm dictionary / Windows checker on a background thread.
pub fn preload(app: AppHandle) {
    std::thread::spawn(move || {
        if let Err(e) = ensure_loaded(&app) {
            eprintln!("[mote] spellcheck preload failed: {e}");
        }
    });
}

fn is_cyrillic(ch: char) -> bool {
    matches!(ch, 'А'..='Я' | 'а'..='я' | 'Ё' | 'ё')
}

fn iter_cyrillic_words(text: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        let is_word = ch.is_ascii_alphabetic() || is_cyrillic(ch);
        if is_word {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            let word = &text[s..i];
            if word.chars().count() >= 3 && word.chars().any(is_cyrillic) {
                out.push((s, i, word));
            }
        }
    }
    if let Some(s) = start {
        let word = &text[s..];
        if word.chars().count() >= 3 && word.chars().any(is_cyrillic) {
            out.push((s, text.len(), word));
        }
    }
    out
}

/// Frequent Russian typos the OS/Hunspell «fix» by edit distance into nonsense
/// (`тоесть` → `доесть`). Prefer these over the engine.
fn common_fix(word: &str) -> Option<&'static str> {
    let key: String = word.chars().flat_map(|c| c.to_lowercase()).collect();
    Some(match key.as_str() {
        "тоесть" => "то есть",
        "вобщем" | "вапще" | "вобще" | "ваобще" => "в общем",
        "изза" => "из-за",
        "какпто" | "както" => "как-то",
        "ктото" => "кто-то",
        "чтото" | "чёто" | "чето" => "что-то",
        "ничево" | "ничо" => "ничего",
        "пожалуста" | "пожалста" => "пожалуйста",
        "здравствуте" | "здраствуйте" => "здравствуйте",
        "превет" | "привед" => "привет",
        "што" => "что",
        // Windows/Hunspell often suggest the name «Нила».
        "дила" => "дела",
        "спасиба" | "спасиб" => "спасибо",
        "извени" | "извинити" => "извини",
        "агенство" => "агентство",
        "инциндент" => "инцидент",
        "учавствовать" => "участвовать",
        "будующего" => "будущего",
        "следуюший" => "следующий",
        "оффис" => "офис",
        "арфографию" => "орфографию",
        "арфография" => "орфография",
        "арфографии" => "орфографии",
        _ => return None,
    })
}

fn match_case(sample: &str, replacement: &str) -> String {
    let mut sample_chars = sample.chars();
    let Some(first) = sample_chars.next() else {
        return replacement.to_string();
    };
    if first.is_uppercase() && sample_chars.all(|c| !c.is_uppercase()) {
        let mut out = String::new();
        let mut repl = replacement.chars();
        if let Some(r0) = repl.next() {
            for c in r0.to_uppercase() {
                out.push(c);
            }
            out.extend(repl);
        }
        return out;
    }
    if sample
        .chars()
        .all(|c| c.is_uppercase() || !c.is_alphabetic())
    {
        return replacement.chars().flat_map(|c| c.to_uppercase()).collect();
    }
    replacement.to_string()
}

fn pick_suggestion(word: &str, engine: Option<String>) -> Option<String> {
    if let Some(fix) = common_fix(word) {
        return Some(match_case(word, fix));
    }
    engine.filter(|s| suggestion_plausible(word, s))
}

/// Drop wild replacements ("вот" for a long unrelated typo, etc.).
fn suggestion_plausible(word: &str, sug: &str) -> bool {
    if sug.is_empty() || sug == word {
        return false;
    }
    // Multi-word fixes only via common_fix (allowed separately).
    if sug.chars().any(|c| c.is_whitespace() || c == '-') {
        return common_fix(word)
            .map(|c| match_case(word, c) == *sug || c == sug)
            .unwrap_or(false);
    }
    if !sug.chars().any(is_cyrillic) {
        return false;
    }
    // Lowercase typo → reject Title-case names («дила» ↛ «Нила»).
    let word_lower = word.chars().all(|c| !c.is_alphabetic() || c.is_lowercase());
    if word_lower {
        let mut sc = sug.chars();
        if let (Some(s0), Some(s1)) = (sc.next(), sc.next()) {
            if s0.is_uppercase() && s1.is_lowercase() {
                return false;
            }
        }
    }
    let wc = word.chars().count();
    let sc = sug.chars().count();
    if sc + 2 < wc.saturating_sub(1) || sc > wc + 3 {
        return false;
    }
    let d = edit_distance(&word.to_lowercase(), &sug.to_lowercase());
    let max_d = match wc {
        0..=3 => 1,
        4..=6 => 2,
        7..=10 => 3,
        _ => 4,
    };
    d > 0 && d <= max_d
}

fn common_typo_hits(text: &str) -> Vec<(usize, usize, String, String)> {
    let mut hits = Vec::new();
    for (start, end, word) in iter_cyrillic_words(text) {
        if crate::spell_ignore::is_ignored(word) {
            continue;
        }
        if let Some(fix) = common_fix(word) {
            hits.push((start, end, word.to_string(), match_case(word, fix)));
        }
    }
    hits
}

/// Engine hits + common typos; common wins on the same span.
fn merge_hits(
    text: &str,
    engine: Vec<(usize, usize, String, String)>,
) -> Vec<(usize, usize, String, String)> {
    let common = common_typo_hits(text);
    let engine: Vec<_> = engine
        .into_iter()
        .filter(|(_, _, word, _)| !crate::spell_ignore::is_ignored(word))
        .collect();
    if common.is_empty() {
        return engine;
    }
    let mut out = common;
    for (start, end, word, sug) in engine {
        if out.iter().any(|(s, e, _, _)| *s == start && *e == end) {
            continue;
        }
        if let Some(fixed) = pick_suggestion(&word, Some(sug)) {
            out.push((start, end, word, fixed));
        }
    }
    out.sort_by_key(|(s, _, _, _)| *s);
    out
}

fn rank_suggestions(word: &str, mut suggestions: Vec<String>) -> Vec<String> {
    let lower = word.to_lowercase();
    let max_len = word.chars().count() + 3;
    suggestions.retain(|s| s.chars().count() <= max_len.max(6) && suggestion_plausible(word, s));
    suggestions.sort_by(|a, b| {
        let da = edit_distance(&lower, &a.to_lowercase());
        let db = edit_distance(&lower, &b.to_lowercase());
        da.cmp(&db)
            .then_with(|| {
                (a.chars().count() as i32 - word.chars().count() as i32)
                    .abs()
                    .cmp(&(b.chars().count() as i32 - word.chars().count() as i32).abs())
            })
            .then_with(|| a.cmp(b))
    });
    suggestions.dedup();
    suggestions
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut cur = vec![0; n + 1];
    for i in 1..=m {
        cur[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (cur[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[n]
}

const RU_LETTERS: &str = "абвгдеёжзийклмнопрстуфхцчшщъыьэюя";

fn edit1_dictionary_hits(dict: &spellbook::Dictionary, word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    if chars.is_empty() || chars.len() > 16 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..chars.len() {
        for repl in RU_LETTERS.chars() {
            if repl == chars[i] {
                continue;
            }
            let mut next = chars.clone();
            next[i] = repl;
            let cand: String = next.into_iter().collect();
            if dict.check(&cand) {
                out.push(cand);
            }
        }
    }
    out
}

fn best_hunspell(dict: &spellbook::Dictionary, word: &str) -> Option<String> {
    if let Some(fix) = common_fix(word) {
        return Some(match_case(word, fix));
    }
    if dict.check(word) {
        return None;
    }
    let mut suggestions = Vec::new();
    dict.suggest(word, &mut suggestions);
    suggestions.extend(edit1_dictionary_hits(dict, word));
    pick_suggestion(word, rank_suggestions(word, suggestions).into_iter().next())
}

/// Convert a UTF-16 code-unit index into a UTF-8 byte index.
fn utf16_to_byte(text: &str, utf16_idx: u32) -> Option<usize> {
    let mut u16s = 0u32;
    for (byte_idx, ch) in text.char_indices() {
        if u16s == utf16_idx {
            return Some(byte_idx);
        }
        u16s += ch.len_utf16() as u32;
        if u16s > utf16_idx {
            return Some(byte_idx);
        }
    }
    if u16s == utf16_idx {
        Some(text.len())
    } else {
        None
    }
}

fn build_payload(
    text: &str,
    hits: Vec<(usize, usize, String, String)>,
) -> Option<SpellOfferPayload> {
    if hits.is_empty() {
        return None;
    }
    let mut replaced = text.to_string();
    for (start, end, _word, sug) in hits.iter().rev() {
        replaced.replace_range(*start..*end, sug);
    }
    let fixes: Vec<SpellFix> = hits
        .iter()
        .map(|(_, _, w, s)| SpellFix {
            word: w.clone(),
            suggestion: s.clone(),
        })
        .collect();
    let bad_joined = fixes
        .iter()
        .map(|f| f.word.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let good_joined = fixes
        .iter()
        .map(|f| f.suggestion.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    // Plaque shows only the fix word(s), not the whole field (Replace still uses `replaced`).
    let (start, end) = (hits[0].0, hits[0].1);
    Some(SpellOfferPayload {
        word: bad_joined,
        suggestions: vec![good_joined.clone(), good_joined],
        start,
        end,
        replaced,
        original: text.to_string(),
        fixes,
    })
}

#[cfg(windows)]
fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { p.to_string().unwrap_or_default() }
}

#[cfg(windows)]
fn free_pwstr(p: windows::core::PWSTR) {
    use windows::Win32::System::Com::CoTaskMemFree;
    if !p.is_null() {
        unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    }
}

#[cfg(windows)]
fn win_suggest(checker: &windows::Win32::Globalization::ISpellChecker, word: &str) -> Vec<String> {
    use windows::core::PCWSTR;
    let wide: Vec<u16> = word.encode_utf16().chain(std::iter::once(0)).collect();
    let Ok(enum_str) = (unsafe { checker.Suggest(PCWSTR(wide.as_ptr())) }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    unsafe {
        loop {
            let mut buf = [windows::core::PWSTR::null()];
            let mut fetched = 0u32;
            let hr = enum_str.Next(&mut buf, Some(&mut fetched));
            if hr.0 != 0 || fetched == 0 || buf[0].is_null() {
                break;
            }
            let s = pwstr_to_string(buf[0]);
            free_pwstr(buf[0]);
            if !s.is_empty() {
                out.push(s);
            }
            if out.len() >= 8 {
                break;
            }
        }
    }
    rank_suggestions(word, out)
}

/// `Ok(None)` = Windows checked and found nothing. `Err(())` = API unavailable / failed.
#[cfg(windows)]
fn offer_via_windows(text: &str) -> Result<Option<SpellOfferPayload>, ()> {
    use windows::core::PCWSTR;
    use windows::Win32::Globalization::{
        CORRECTIVE_ACTION_GET_SUGGESTIONS, CORRECTIVE_ACTION_REPLACE,
    };

    with_windows_checker(|checker| {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let errors = match unsafe { checker.Check(PCWSTR(wide.as_ptr())) } {
            Ok(e) => e,
            Err(_) => return Err(()),
        };

        let mut hits: Vec<(usize, usize, String, String)> = Vec::new();
        unsafe {
            loop {
                let mut err = None;
                let hr = errors.Next(&mut err);
                if hr.0 != 0 {
                    break;
                }
                let Some(err) = err else { break };

                let start_u16 = err.StartIndex().unwrap_or(0);
                let len_u16 = err.Length().unwrap_or(0);
                if len_u16 == 0 {
                    continue;
                }
                let Some(start) = utf16_to_byte(text, start_u16) else {
                    continue;
                };
                let Some(end) = utf16_to_byte(text, start_u16 + len_u16) else {
                    continue;
                };
                if end <= start || end > text.len() {
                    continue;
                }
                let word = &text[start..end];
                if word.chars().count() < 3 || !word.chars().any(is_cyrillic) {
                    continue;
                }
                if crate::spell_ignore::is_ignored(word) {
                    continue;
                }

                let action = err
                    .CorrectiveAction()
                    .unwrap_or(CORRECTIVE_ACTION_GET_SUGGESTIONS);
                let engine = if action == CORRECTIVE_ACTION_REPLACE {
                    let repl = err.Replacement().ok();
                    let s = repl
                        .as_ref()
                        .map(|p| pwstr_to_string(*p))
                        .unwrap_or_default();
                    if let Some(p) = repl {
                        free_pwstr(p);
                    }
                    if s.is_empty() {
                        win_suggest(checker, word).into_iter().next()
                    } else {
                        Some(s)
                    }
                } else {
                    win_suggest(checker, word).into_iter().next()
                };

                if let Some(sug) = pick_suggestion(word, engine) {
                    hits.push((start, end, word.to_string(), sug));
                }
            }
        }

        Ok(build_payload(text, merge_hits(text, hits)))
    })
    .unwrap_or(Err(()))
}

fn offer_via_hunspell(dict: &spellbook::Dictionary, text: &str) -> Option<SpellOfferPayload> {
    let words = iter_cyrillic_words(text);
    let mut hits = Vec::new();
    for (start, end, word) in words {
        if crate::spell_ignore::is_ignored(word) {
            continue;
        }
        if let Some(sug) = best_hunspell(dict, word) {
            hits.push((start, end, word.to_string(), sug));
        }
    }
    build_payload(text, merge_hits(text, hits))
}

/// Check the whole sentence / field and correct every misspelled Cyrillic word.
pub fn spell_offer(app: AppHandle, text: String) -> Result<Option<SpellOfferPayload>, String> {
    if text.trim().is_empty() || !text.chars().any(is_cyrillic) {
        return Ok(None);
    }
    // Common typos work even if Windows/Hunspell dictionaries are missing.
    if let Err(e) = ensure_loaded(&app) {
        return Ok(build_payload(&text, common_typo_hits(&text)).or_else(|| {
            eprintln!("[mote] spellcheck load failed: {e}");
            None
        }));
    }

    #[cfg(windows)]
    {
        match offer_via_windows(&text) {
            Ok(payload) => return Ok(payload),
            Err(()) => {
                // Fall through to Hunspell.
            }
        }
    }

    let slot = spell_slot();
    let guard = slot.lock().map_err(|_| "spell lock".to_string())?;
    let state = guard
        .as_ref()
        .ok_or_else(|| "словарь не загружен".to_string())?;
    if let Some(payload) = state
        .hunspell
        .as_ref()
        .and_then(|d| offer_via_hunspell(d, &text))
    {
        return Ok(Some(payload));
    }
    Ok(build_payload(&text, common_typo_hits(&text)))
}

pub fn spell_ready(_app: AppHandle) -> bool {
    spell_slot()
        .lock()
        .map(|g| {
            g.as_ref()
                .map(|s| {
                    #[cfg(windows)]
                    {
                        s.win_available || s.hunspell.is_some()
                    }
                    #[cfg(not(windows))]
                    {
                        s.hunspell.is_some()
                    }
                })
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// True if `word` is in the en_US Hunspell dictionary (layout_auto: leave English alone).
pub fn english_word_known(app: &AppHandle, word: &str) -> bool {
    let w = word.trim();
    if w.chars().count() < 2 || !w.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let _ = ensure_loaded(app);
    let Ok(guard) = spell_slot().lock() else {
        return false;
    };
    let Some(state) = guard.as_ref() else {
        return false;
    };
    let Some(dict) = state.hunspell_en.as_ref() else {
        return false;
    };
    dict.check(w) || dict.check(&w.to_lowercase())
}

/// Single Cyrillic token (≥2) known to Hunspell / common_fix — for short layout words (ты, не).
pub fn cyrillic_word_known(app: &AppHandle, word: &str) -> bool {
    let w = word.trim();
    if w.chars().count() < 2 || !w.chars().any(is_cyrillic) {
        return false;
    }
    if common_fix(w).is_some() {
        return true;
    }
    let _ = ensure_loaded(app);
    let Ok(guard) = spell_slot().lock() else {
        return false;
    };
    let Some(state) = guard.as_ref() else {
        return false;
    };
    let Some(dict) = state.hunspell.as_ref() else {
        return false;
    };
    dict.check(w) || dict.check(&w.to_lowercase())
}

/// Near-miss Cyrillic fix for layout_auto (`превет` → `привет`, `што` → `что`).
pub fn suggest_cyrillic_near(app: &AppHandle, word: &str) -> Option<String> {
    let w = word.trim();
    if w.chars().count() < 2 || !w.chars().any(is_cyrillic) {
        return None;
    }
    if let Some(fix) = common_fix(w) {
        return Some(match_case(w, fix));
    }
    let _ = ensure_loaded(app);
    let Ok(guard) = spell_slot().lock() else {
        return None;
    };
    let Some(state) = guard.as_ref() else {
        return None;
    };
    let Some(dict) = state.hunspell.as_ref() else {
        return None;
    };
    if dict.check(w) || dict.check(&w.to_lowercase()) {
        return Some(w.to_string());
    }
    best_hunspell(dict, w)
}

/// `(known, total)` Cyrillic tokens (≥3 letters) recognized by Hunspell.
/// Used by layout_fix EN→RU scoring.
pub fn cyrillic_known_score(app: &AppHandle, text: &str) -> (usize, usize) {
    let _ = ensure_loaded(app);
    let Ok(guard) = spell_slot().lock() else {
        return (0, 0);
    };
    let Some(state) = guard.as_ref() else {
        return (0, 0);
    };
    let dict = state.hunspell.as_ref();
    let mut known = 0usize;
    let mut total = 0usize;
    for (_s, _e, word) in iter_cyrillic_words(text) {
        total += 1;
        let ok = dict
            .map(|d| d.check(word) || d.check(&word.to_lowercase()))
            .unwrap_or(false);
        if ok {
            known += 1;
        }
    }
    (known, total)
}

/// Part of the field live-spell should check (not the whole document).
#[derive(Clone, Debug)]
pub struct LiveSpellScope {
    pub prefix: String,
    pub delta: String,
    pub suffix: String,
}

fn is_spell_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || (ch.is_ascii_punctuation() && ch != '-' && ch != '\'')
        || "«»…—–".contains(ch)
}

/// Split `full` into prefix/delta/suffix relative to `baseline` (text at compose start).
pub fn live_spell_scope(baseline: &str, full: &str) -> LiveSpellScope {
    let b: Vec<char> = baseline.chars().collect();
    let f: Vec<char> = full.chars().collect();
    let mut i = 0usize;
    while i < b.len() && i < f.len() && b[i] == f[i] {
        i += 1;
    }
    if i < b.len() {
        // Mid-document edit relative to baseline — live-spell stays off (use selection).
        return LiveSpellScope {
            prefix: full.to_string(),
            delta: String::new(),
            suffix: String::new(),
        };
    }
    LiveSpellScope {
        prefix: f[..i].iter().collect(),
        delta: f[i..].iter().collect(),
        suffix: String::new(),
    }
}

/// True when delta ends on a word boundary (space/punct) — avoids `смо→само` mid-type.
pub fn delta_ready_for_live_spell(delta: &str) -> bool {
    let t = delta.trim();
    if t.is_empty() || !t.chars().any(is_cyrillic) {
        return false;
    }
    delta.chars().last().is_some_and(is_spell_boundary)
}

/// After a longer idle: user stopped on a finished Cyrillic word (no trailing space).
pub fn delta_ready_after_idle(delta: &str) -> bool {
    if delta_ready_for_live_spell(delta) {
        return true;
    }
    let t = delta.trim();
    if t.is_empty() || !t.chars().any(is_cyrillic) {
        return false;
    }
    // Last run of letters must be the end of delta (still typing mid-word → false).
    let last = t
        .chars()
        .rev()
        .take_while(|c| is_cyrillic(*c) || *c == '-')
        .collect::<String>();
    let last: String = last.chars().rev().collect();
    last.chars().count() >= 4 && t.ends_with(&last)
}

/// True while the user is still extending a word (suppress mid-token plaques).
pub fn delta_mid_word(delta: &str) -> bool {
    if delta.is_empty() {
        return false;
    }
    !delta.chars().last().is_some_and(is_spell_boundary)
}

/// Offered typo is still a whole token — keep the plaque while the next word is typed.
pub fn offered_word_intact(text: &str, word: &str) -> bool {
    let w = word.trim();
    if w.is_empty() {
        return false;
    }
    let text_l = text.to_lowercase();
    let w_l = w.to_lowercase();
    let mut from = 0usize;
    while let Some(rel) = text_l[from..].find(&w_l) {
        let i = from + rel;
        let end = i + w_l.len();
        let before_ok = text_l[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphabetic());
        let after_ok = text_l[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphabetic());
        if before_ok && after_ok {
            return true;
        }
        from = end;
        if from >= text_l.len() {
            break;
        }
    }
    false
}

/// Remap a delta-sized offer onto the full field for Replace.
pub fn apply_scope_to_offer(
    mut offer: SpellOfferPayload,
    scope: &LiveSpellScope,
) -> SpellOfferPayload {
    let prefix_bytes = scope.prefix.len();
    offer.start = offer.start.saturating_add(prefix_bytes);
    offer.end = offer.end.saturating_add(prefix_bytes);
    offer.replaced = format!("{}{}{}", scope.prefix, offer.replaced, scope.suffix);
    if offer.original.is_empty() {
        offer.original = format!("{}{}{}", scope.prefix, scope.delta, scope.suffix);
    } else {
        offer.original = format!("{}{}{}", scope.prefix, offer.original, scope.suffix);
    }
    offer
}

#[cfg(test)]
mod live_scope_tests {
    use super::*;

    #[test]
    fn append_only_delta() {
        let s = live_spell_scope("hello ", "hello превет ");
        assert_eq!(s.prefix, "hello ");
        assert_eq!(s.delta, "превет ");
        assert!(delta_ready_for_live_spell(&s.delta));
    }

    #[test]
    fn mid_edit_skips_live() {
        let s = live_spell_scope("abcdef", "abXdef");
        assert!(s.delta.is_empty());
        assert!(!delta_ready_for_live_spell(&s.delta));
    }

    #[test]
    fn mid_word_not_ready() {
        assert!(!delta_ready_for_live_spell("превет"));
        assert!(delta_ready_for_live_spell("превет "));
        assert!(delta_ready_after_idle("превет"));
        assert!(!delta_ready_after_idle("пре"));
    }

    #[test]
    fn apply_scope_joins() {
        let scope = LiveSpellScope {
            prefix: "aa ".into(),
            delta: "bb ".into(),
            suffix: String::new(),
        };
        let offer = SpellOfferPayload {
            word: "bb".into(),
            suggestions: vec!["cc".into()],
            start: 0,
            end: 2,
            replaced: "cc ".into(),
            original: "bb ".into(),
            fixes: vec![],
        };
        let full = apply_scope_to_offer(offer, &scope);
        assert_eq!(full.replaced, "aa cc ");
        assert_eq!(full.original, "aa bb ");
    }

    #[test]
    fn toest_not_doest() {
        assert_eq!(common_fix("тоесть"), Some("то есть"));
        assert_eq!(
            pick_suggestion("тоесть", Some("доесть".into())).as_deref(),
            Some("то есть")
        );
        let hits = common_typo_hits("ну тоесть курсор");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].3, "то есть");
    }

    #[test]
    fn dila_is_dela_not_nila() {
        assert_eq!(common_fix("дила"), Some("дела"));
        assert_eq!(
            pick_suggestion("дила", Some("Нила".into())).as_deref(),
            Some("дела")
        );
        assert!(!suggestion_plausible("дила", "Нила"));
    }
}
