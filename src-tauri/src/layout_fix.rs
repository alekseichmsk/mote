//! QWERTY ↔ ЙЦУКЕН layout flip for mistyped Russian/English.

use std::sync::atomic::{AtomicBool, Ordering};

const MAX_CHARS: usize = 2000;

static AUTO_ENABLED: AtomicBool = AtomicBool::new(false);

/// Lowercase / uppercase pairs: US QWERTY key → Russian letter (same physical key).
const EN_TO_RU: &[(char, char)] = &[
    ('q', 'й'),
    ('w', 'ц'),
    ('e', 'у'),
    ('r', 'к'),
    ('t', 'е'),
    ('y', 'н'),
    ('u', 'г'),
    ('i', 'ш'),
    ('o', 'щ'),
    ('p', 'з'),
    ('[', 'х'),
    (']', 'ъ'),
    ('a', 'ф'),
    ('s', 'ы'),
    ('d', 'в'),
    ('f', 'а'),
    ('g', 'п'),
    ('h', 'р'),
    ('j', 'о'),
    ('k', 'л'),
    ('l', 'д'),
    (';', 'ж'),
    ('\'', 'э'),
    ('z', 'я'),
    ('x', 'ч'),
    ('c', 'с'),
    ('v', 'м'),
    ('b', 'и'),
    ('n', 'т'),
    ('m', 'ь'),
    (',', 'б'),
    ('.', 'ю'),
    ('`', 'ё'),
    ('Q', 'Й'),
    ('W', 'Ц'),
    ('E', 'У'),
    ('R', 'К'),
    ('T', 'Е'),
    ('Y', 'Н'),
    ('U', 'Г'),
    ('I', 'Ш'),
    ('O', 'Щ'),
    ('P', 'З'),
    ('{', 'Х'),
    ('}', 'Ъ'),
    ('A', 'Ф'),
    ('S', 'Ы'),
    ('D', 'В'),
    ('F', 'А'),
    ('G', 'П'),
    ('H', 'Р'),
    ('J', 'О'),
    ('K', 'Л'),
    ('L', 'Д'),
    (':', 'Ж'),
    ('"', 'Э'),
    ('Z', 'Я'),
    ('X', 'Ч'),
    ('C', 'С'),
    ('V', 'М'),
    ('B', 'И'),
    ('N', 'Т'),
    ('M', 'Ь'),
    ('<', 'Б'),
    ('>', 'Ю'),
    ('~', 'Ё'),
    ('/', '.'),
    ('?', ','),
];

fn map_char(ch: char, en_to_ru: bool) -> char {
    if en_to_ru {
        for &(en, ru) in EN_TO_RU {
            if ch == en {
                return ru;
            }
        }
        ch
    } else {
        for &(en, ru) in EN_TO_RU {
            if ch == ru {
                return en;
            }
        }
        ch
    }
}

pub fn convert_en_to_ru(text: &str) -> String {
    text.chars().map(|c| map_char(c, true)).collect()
}

pub fn convert_ru_to_en(text: &str) -> String {
    text.chars().map(|c| map_char(c, false)).collect()
}

fn is_cyrillic(ch: char) -> bool {
    matches!(ch, 'А'..='Я' | 'а'..='я' | 'Ё' | 'ё')
}

fn count_scripts(text: &str) -> (usize, usize) {
    let mut latin = 0usize;
    let mut cyr = 0usize;
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            latin += 1;
        } else if is_cyrillic(ch) {
            cyr += 1;
        }
    }
    (latin, cyr)
}

/// True when the *entire* field is a URL/code blob (skip all). Mixed «текст + https://…»
/// must NOT match — URL spans skip path tokens in `try_auto_replace`.
fn looks_like_url_or_code(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Paste of a single URL / git remote (no spaces).
    if !trimmed.chars().any(|c| c.is_whitespace())
        && (trimmed.contains("://")
            || trimmed.starts_with("www.")
            || trimmed.starts_with("git@")
            || trimmed.ends_with(".git"))
    {
        return true;
    }
    // Multi-word prose (+ optional link) → never skip the whole field.
    if trimmed.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    looks_like_url_token(trimmed)
        || trimmed.contains('\\')
        || trimmed.contains('{')
        || trimmed.contains('}')
}

/// Byte ranges of URL / git-remote runs (ASCII). Tokens inside must not EN→RU.
fn url_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let n = text.len();
    let mut i = 0usize;
    while i < n {
        let rest = &text[i..];
        let rest_l = rest.to_ascii_lowercase();
        let hit = rest_l.starts_with("https://")
            || rest_l.starts_with("http://")
            || rest_l.starts_with("www.")
            || rest_l.starts_with("git@");
        if hit {
            let mut end = i;
            for ch in text[i..].chars() {
                if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'' | '«' | '»' | '(' | ')')
                {
                    break;
                }
                end += ch.len_utf8();
            }
            if end > i {
                spans.push((i, end));
                i = end;
                continue;
            }
        }
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    spans
}

fn ranges_overlap(a0: usize, a1: usize, b0: usize, b1: usize) -> bool {
    a0 < b1 && a1 > b0
}

/// Skip layout on URL-like Latin runs (`rive.app`, `https://…`, `foo.com/bar`, `mote.git`).
fn looks_like_url_token(word: &str) -> bool {
    let w = word.trim();
    if w.is_empty() {
        return false;
    }
    let lower = w.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") || lower.starts_with("git@") {
        return true;
    }
    // Repo suffix when `/` split the URL (`mote.git`).
    if lower.ends_with(".git") {
        return true;
    }
    // Scheme crumbs when `/` splits `https://…` into tokens (`https:` / `https`).
    let scheme = lower.trim_end_matches(':');
    if matches!(scheme, "http" | "https" | "ftp" | "mailto" | "git") {
        return true;
    }
    if lower.contains('/') && lower.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3 {
        return true;
    }
    // domain.tld… (`.` is also a RU letter key — must not flip `rive.app`)
    let Some(dot) = lower.rfind('.') else {
        return false;
    };
    if dot == 0 || dot + 1 >= lower.len() {
        return false;
    }
    let host = &lower[..dot];
    let rest = &lower[dot + 1..];
    if !host.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let tld: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    matches!(
        tld.as_str(),
        "com"
            | "ru"
            | "org"
            | "net"
            | "io"
            | "app"
            | "dev"
            | "me"
            | "info"
            | "xyz"
            | "ai"
            | "cc"
            | "tv"
            | "to"
            | "co"
            | "uk"
            | "de"
            | "fr"
            | "es"
            | "it"
            | "pl"
            | "ua"
            | "by"
            | "kz"
            | "su"
            | "edu"
            | "gov"
            | "mil"
            | "pro"
            | "site"
            | "online"
            | "tech"
            | "shop"
            | "blog"
            | "cloud"
            | "page"
            | "link"
            | "gg"
            | "fm"
            | "ly"
            | "git"
    )
}

/// True for intentional abbreviations: `OK`, `PDF`, `API` (2+ ASCII letters, all caps).
pub fn is_all_caps_abbrev(word: &str) -> bool {
    let core = ascii_alpha_core(word);
    let n = core.chars().count();
    n >= 2 && core.chars().all(|c| c.is_ascii_uppercase())
}

fn should_skip_latin_token(app: &tauri::AppHandle, word: &str, mixed_field: bool) -> bool {
    let core = ascii_alpha_core(word);
    if core.is_empty() {
        return false;
    }
    if looks_like_url_token(word) {
        eprintln!("[mote] layout skip url «{word}»");
        return true;
    }
    if crate::spell_ignore::is_ignored(core) || crate::spell_ignore::is_ignored(word) {
        eprintln!("[mote] layout skip ignore «{word}»");
        return true;
    }
    if is_all_caps_abbrev(word) {
        eprintln!("[mote] layout skip caps «{word}»");
        return true;
    }
    let en_known = crate::spellcheck::english_word_known(app, core);
    // Mixed: only trust EN dict for ≥3 letters (keeps bot/hello; frees ye/jr).
    en_known && (!mixed_field || core.chars().count() >= 3)
}

/// Button / force: only Latin runs → RU. Existing Cyrillic untouched (no «а»→«ф»).
pub fn flip_latin_preserving_cyrillic(app: &tauri::AppHandle, text: &str) -> Option<String> {
    if text.trim().is_empty() || text.chars().count() > MAX_CHARS {
        return None;
    }
    if looks_like_url_or_code(text) {
        eprintln!("[mote] layout_fix skip url/code");
        return None;
    }
    let words = latin_words_in(text);
    if words.is_empty() {
        return None;
    }
    let spans = url_spans(text);
    let force = field_has_cyrillic(text) || words.len() >= 2;
    let mut out = text.to_string();
    let mut n = 0usize;
    for (start, word_end, word) in words.into_iter().rev() {
        if spans
            .iter()
            .any(|&(s, e)| ranges_overlap(start, word_end, s, e))
        {
            eprintln!("[mote] layout_fix skip url-span «{word}»");
            continue;
        }
        if should_skip_latin_token(app, word, force) {
            continue;
        }
        let converted = convert_en_to_ru(word);
        if converted == word || !converted.chars().any(is_cyrillic) {
            continue;
        }
        eprintln!("[mote] layout_fix en2ru «{word}»→«{converted}»");
        out.replace_range(start..word_end, &converted);
        n += 1;
    }
    if n == 0 {
        eprintln!("[mote] layout_fix noop chars={}", text.chars().count());
        return None;
    }
    activate_russian_keyboard();
    eprintln!(
        "[mote] layout_fix ok dir=en2ru chars={} n={n}",
        out.chars().count()
    );
    Some(out)
}

/// Flip Latin-majority text EN→RU (unit tests). No RU→EN (avoids «а»→«ф»).
pub fn flip_by_script(text: &str) -> Option<String> {
    if text.trim().is_empty() || text.chars().count() > MAX_CHARS {
        return None;
    }
    if looks_like_url_or_code(text) {
        eprintln!("[mote] layout_fix skip url/code");
        return None;
    }
    let (latin, cyr) = count_scripts(text);
    if latin == 0 {
        return None;
    }
    if cyr > latin {
        return None;
    }
    let out = convert_en_to_ru(text);
    if out == text {
        eprintln!("[mote] layout_fix noop chars={}", text.chars().count());
        return None;
    }
    eprintln!(
        "[mote] layout_fix ok dir=en2ru chars={}",
        out.chars().count()
    );
    activate_russian_keyboard();
    Some(out)
}

/// Auto EN→RU when Hunspell recognizes the converted Cyrillic words.
pub fn suggest_en_to_ru(app: &tauri::AppHandle, text: &str) -> Option<String> {
    if text.trim().is_empty() || text.chars().count() > MAX_CHARS {
        return None;
    }
    if looks_like_url_or_code(text) {
        eprintln!("[mote] layout_fix skip url/code");
        return None;
    }
    let (latin, cyr) = count_scripts(text);
    if latin < 3 || latin <= cyr {
        return None;
    }
    // Token-wise so `PDF` / ignore stay; only wrong-layout Latin flips.
    let converted = flip_latin_preserving_cyrillic(app, text)?;
    let (known, total) = crate::spellcheck::cyrillic_known_score(app, &converted);
    let raw = if total == 1 && known == 1 {
        eprintln!(
            "[mote] layout_fix ok dir=en2ru chars={} known=1/1",
            converted.chars().count()
        );
        converted
    } else if total == 1 {
        let fix = crate::spellcheck::suggest_cyrillic_near(app, &converted)?;
        eprintln!(
            "[mote] layout_fix ok dir=en2ru fuzzy chars={}",
            fix.chars().count()
        );
        fix
    } else if total > 1 && known >= 1 && known * 2 >= total {
        eprintln!(
            "[mote] layout_fix ok dir=en2ru chars={} known={known}/{total}",
            converted.chars().count()
        );
        converted
    } else {
        eprintln!(
            "[mote] layout_fix skip score known={known} total={total} chars={}",
            text.chars().count()
        );
        return None;
    };
    Some(raw)
}

/// Polish one newly converted token only (`превет`→`привет`), never the whole field.
fn polish_converted_token(app: &tauri::AppHandle, token: &str) -> String {
    // Short layout hits (`jr`→`ок`, `ns`→`ты`) — Hunspell often «fixes» ок→об.
    if token.chars().count() <= 2 {
        return token.to_string();
    }
    if crate::spellcheck::cyrillic_word_known(app, token) {
        return token.to_string();
    }
    match crate::spellcheck::suggest_cyrillic_near(app, token) {
        Some(fix) if fix != token => {
            eprintln!("[mote] layout_auto token-polish «{token}»→«{fix}»");
            fix
        }
        _ => token.to_string(),
    }
}

/// `force`: button — Latin→RU only, keep Cyrillic / caps / ignore. Else scored EN→RU.
pub fn layout_fix(app: &tauri::AppHandle, text: String, force: bool) -> Option<String> {
    if force {
        flip_latin_preserving_cyrillic(app, &text)
    } else {
        suggest_en_to_ru(app, &text)
    }
}

pub fn init_auto_from_prefs() {
    let on = crate::llm::load_layout_auto();
    AUTO_ENABLED.store(on, Ordering::Relaxed);
    eprintln!("[mote] layout_auto {}", if on { "on" } else { "off" });
}

pub fn is_auto_enabled() -> bool {
    AUTO_ENABLED.load(Ordering::Relaxed)
}

pub fn set_auto_enabled(enabled: bool) -> Result<(), String> {
    crate::llm::set_layout_auto(enabled)?;
    AUTO_ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

/// Switch Windows input language to Russian (after EN→RU text fix).
pub fn activate_russian_keyboard() {
    #[cfg(windows)]
    {
        use windows::core::w;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            ActivateKeyboardLayout, LoadKeyboardLayoutW, KLF_ACTIVATE,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, PostMessageW, WM_INPUTLANGCHANGEREQUEST,
        };

        unsafe {
            let hkl = match LoadKeyboardLayoutW(w!("00000419"), KLF_ACTIVATE) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("[mote] layout kb LoadKeyboardLayout ru: {e}");
                    return;
                }
            };
            let _ = ActivateKeyboardLayout(hkl, Default::default());
            let hwnd = GetForegroundWindow();
            if !hwnd.0.is_null() {
                // Per-window layout (Cursor / Chrome) often needs this.
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_INPUTLANGCHANGEREQUEST,
                    WPARAM(0),
                    LPARAM(hkl.0 as isize),
                );
            }
            eprintln!("[mote] layout kb → ru-RU");
        }
    }
}

fn is_word_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            // `/` `?` = RU `.` `,` when typed on EN. Do NOT treat `,` `.` `[` as
            // boundaries here for token cuts — those keys are RU letters б ю х.
            '!' | '?' | '/' | '…' | '—' | '–' | '(' | ')' | '"' | '«' | '»'
        )
}

/// EN key that produces a Russian *letter* (not sentence punct `/` `?`).
fn is_layout_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic()
        || matches!(
            ch,
            ',' | '.' | '[' | ']' | ';' | '\'' | '`' | '{' | '}' | ':' | '"' | '<' | '>' | '~'
        )
}

/// EN key → RU glyph when the user meant Russian punctuation on a Latin layout.
fn en_punct_to_ru(ch: char) -> Option<char> {
    match ch {
        '/' => Some('.'),
        '?' => Some(','),
        _ => None,
    }
}

/// Content end index: strip trailing whitespace/punct boundaries.
fn content_end_before_boundaries(text: &str) -> Option<usize> {
    let last = text.chars().last()?;
    if !is_word_boundary(last) {
        return None;
    }
    let mut end = text.len();
    for (i, ch) in text.char_indices().rev() {
        if is_word_boundary(ch) {
            end = i;
        } else {
            break;
        }
    }
    Some(end)
}

/// Latin / layout-letter tokens (incl. `,sdftn`→будет, `[kt,`→хлеб).
/// Pure punct (`.` `,` alone) is NOT a token — real sentence marks after Cyrillic.
fn latin_words_in(text: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        if is_layout_letter(ch) {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            let word = &text[s..i];
            // Need ≥1 ASCII letter so «ок.» does not flip `.`→«ю», `,`→«б».
            if word.chars().any(|c| c.is_ascii_alphabetic()) {
                out.push((s, i, word));
            }
        }
    }
    if let Some(s) = start {
        let word = &text[s..];
        if word.chars().any(|c| c.is_ascii_alphabetic()) {
            out.push((s, text.len(), word));
        }
    }
    out
}

/// Last pure-Latin token immediately before a trailing word boundary.
pub fn last_latin_token_before_boundary(text: &str) -> Option<(usize, usize, &str)> {
    let end = content_end_before_boundaries(text)?;
    if end == 0 {
        return None;
    }
    latin_words_in(&text[..end]).into_iter().last()
}

/// Mode A: delta has Latin layout tokens (boundary preferred; idle may omit it).
pub fn delta_ready_for_layout_auto(delta: &str) -> bool {
    if !is_auto_enabled() {
        return false;
    }
    if let Some(end) = content_end_before_boundaries(delta) {
        return !latin_words_in(&delta[..end]).is_empty();
    }
    // No trailing space yet — still ready if there is something to convert (idle gate in watcher).
    !latin_words_in(delta).is_empty()
}

/// True when delta ends on a layout word-boundary (space / ! / ? / /).
pub fn delta_has_layout_boundary(delta: &str) -> bool {
    content_end_before_boundaries(delta).is_some()
}

fn field_has_cyrillic(text: &str) -> bool {
    text.chars().any(is_cyrillic)
}

fn ascii_alpha_core(word: &str) -> &str {
    let bytes = word.as_bytes();
    let mut a = 0usize;
    while a < bytes.len() && !bytes[a].is_ascii_alphabetic() {
        a += 1;
    }
    let mut b = bytes.len();
    while b > a && !bytes[b - 1].is_ascii_alphabetic() {
        b -= 1;
    }
    &word[a..b]
}

/// Map Latin token → Russian letters.
/// Skip: ignore list, ALL-CAPS abbr, real English (`hello` / `bot`).
fn resolve_layout_word(app: &tauri::AppHandle, word: &str, mixed_field: bool) -> Option<String> {
    if should_skip_latin_token(app, word, mixed_field) {
        if !is_all_caps_abbrev(word)
            && !crate::spell_ignore::is_ignored(ascii_alpha_core(word))
            && !crate::spell_ignore::is_ignored(word)
        {
            eprintln!("[mote] layout_auto skip en «{word}»");
        }
        return None;
    }
    let converted = convert_en_to_ru(word);
    if converted == word || !converted.chars().any(is_cyrillic) {
        return None;
    }
    // Russian already in the field → convert leftover Latin (rfr→как after «…?»).
    if mixed_field {
        eprintln!("[mote] layout_auto force-mixed «{word}»→«{converted}»");
        return Some(converted);
    }
    // Exact RU including short words (ты, не).
    if crate::spellcheck::cyrillic_word_known(app, &converted) {
        return Some(converted);
    }
    // «ок» / «ты» often missing from Hunspell → fuzzy invents «об»/«гл».
    if converted.chars().count() <= 2 {
        eprintln!("[mote] layout_auto short «{word}»→«{converted}»");
        return Some(converted);
    }
    // Latin-only field: fuzzy (што→что, превет→привет).
    if let Some(fix) = crate::spellcheck::suggest_cyrillic_near(app, &converted) {
        if fix != converted {
            eprintln!("[mote] layout_auto fuzzy «{word}»→«{converted}»→«{fix}»");
        }
        return Some(fix);
    }
    eprintln!("[mote] layout_auto skip «{word}»→«{converted}»");
    None
}

/// If auto-on — convert every Latin token that maps to Russian (not only the last).
pub fn try_auto_replace(
    app: &tauri::AppHandle,
    full_text: &str,
) -> Option<(String, String, String)> {
    if !is_auto_enabled() {
        return None;
    }
    if full_text.chars().count() > MAX_CHARS || looks_like_url_or_code(full_text) {
        return None;
    }
    // Prefer closed phrase (trailing boundary); else whole field (idle stop mid-last-word ok).
    let slice_end = content_end_before_boundaries(full_text).unwrap_or(full_text.len());
    if slice_end == 0 {
        return None;
    }
    let words = latin_words_in(&full_text[..slice_end]);
    if words.is_empty() {
        return None;
    }
    let spans = url_spans(full_text);
    let mut out = full_text.to_string();
    let mut first_from = String::new();
    let mut last_to = String::new();
    let mut n = 0usize;
    // Any Cyrillic in the whole field → force-convert Latin (real EN still skipped).
    let mixed_field = field_has_cyrillic(&out);
    // Phrase with several Latin tokens → treat like mixed (ye/jr/f + dict misses).
    let multi_latin = words.len() >= 2;
    let force = mixed_field || multi_latin;
    // Right-to-left so byte ranges stay valid after each replace.
    for (start, word_end, word) in words.into_iter().rev() {
        if spans
            .iter()
            .any(|&(s, e)| ranges_overlap(start, word_end, s, e))
        {
            eprintln!("[mote] layout_auto skip url-span «{word}»");
            continue;
        }
        let Some(mut fixed) = resolve_layout_word(app, word, force) else {
            continue;
        };
        // Only the converted token — do not spell-fix earlier Cyrillic («хмм»).
        fixed = polish_converted_token(app, &fixed);
        let mut replace_end = word_end;
        if let Some(ch) = out[word_end..].chars().next() {
            if let Some(ru) = en_punct_to_ru(ch) {
                replace_end = word_end + ch.len_utf8();
                fixed.push(ru);
            }
        }
        eprintln!("[mote] layout_auto ok «{word}»→«{fixed}»");
        if first_from.is_empty() {
            first_from = word.to_string();
        }
        last_to = fixed.clone();
        out.replace_range(start..replace_end, &fixed);
        n += 1;
    }
    if n == 0 {
        return None;
    }
    activate_russian_keyboard();
    Some((out, first_from, last_to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghbdtn_to_privet() {
        assert_eq!(convert_en_to_ru("ghbdtn"), "привет");
        assert_eq!(convert_en_to_ru("Ghbdtn"), "Привет");
    }

    #[test]
    fn privet_to_ghbdtn() {
        assert_eq!(convert_ru_to_en("привет"), "ghbdtn");
    }

    #[test]
    fn flip_latin_majority() {
        assert_eq!(flip_by_script("ghbdtn").as_deref(), Some("привет"));
    }

    #[test]
    fn skip_url() {
        assert_eq!(flip_by_script("https://example.com"), None);
    }

    #[test]
    fn preserves_spaces() {
        assert_eq!(convert_en_to_ru("ghbdtn vbh"), "привет мир");
    }

    #[test]
    fn punctuation_keys() {
        assert_eq!(convert_en_to_ru("ghbdtn/"), "привет.");
        assert_eq!(convert_en_to_ru("ghbdtn?"), "привет,");
        assert_eq!(convert_en_to_ru("ghbdtn."), "приветю");
        assert_eq!(convert_ru_to_en("привет."), "ghbdtn/");
    }

    #[test]
    fn flip_keeps_edges() {
        assert_eq!(flip_by_script(" ghbdtn ").as_deref(), Some(" привет "));
    }

    #[test]
    fn hello_converts_but_is_not_russian_word() {
        // Autocomplete path rejects via Hunspell score; map alone is deterministic.
        assert_eq!(convert_en_to_ru("hello"), "руддщ");
    }

    #[test]
    fn last_latin_token_after_space() {
        let (s, e, w) = last_latin_token_before_boundary("xx ghbdtn ").unwrap();
        assert_eq!(w, "ghbdtn");
        assert_eq!(&"xx ghbdtn "[s..e], "ghbdtn");
    }

    #[test]
    fn last_latin_requires_boundary() {
        assert!(last_latin_token_before_boundary("ghbdtn").is_none());
    }

    #[test]
    fn slash_is_boundary_for_ru_period() {
        let (s, e, w) = last_latin_token_before_boundary("ghbdtn/").unwrap();
        assert_eq!(w, "ghbdtn");
        assert_eq!(&"ghbdtn/"[s..e], "ghbdtn");
        assert_eq!(en_punct_to_ru('/'), Some('.'));
    }

    #[test]
    fn short_words_and_phrase() {
        assert_eq!(convert_en_to_ru("ns"), "ты");
        assert_eq!(convert_en_to_ru("yt"), "не");
        assert_eq!(convert_en_to_ru("dct"), "все");
        assert_eq!(convert_en_to_ru("inj"), "што");
        let words = latin_words_in("inj ns nfv");
        assert_eq!(words.len(), 3);
        assert_eq!(content_end_before_boundaries("inj ns nfv ").unwrap(), 10);
        assert!(!latin_words_in("inj ns nfv").is_empty());
    }

    #[test]
    fn mixed_field_detects_one_cyrillic() {
        assert!(field_has_cyrillic("привет aru;bot "));
        assert!(!field_has_cyrillic("hello bot "));
    }

    #[test]
    fn mixed_field_latin_tokens_split_on_semicolon() {
        // `;` is RU «ж», so it sticks to the Latin run; EN core «bot» still detectable.
        let words = latin_words_in("привет ару;bot ");
        assert_eq!(
            words.iter().map(|(_, _, w)| *w).collect::<Vec<_>>(),
            vec![";bot"]
        );
        assert_eq!(ascii_alpha_core(";bot"), "bot");
        assert_eq!(convert_en_to_ru("bot"), "ище");
        assert_eq!(convert_en_to_ru("fhe"), "ару");
    }

    #[test]
    fn short_en_false_positives_map_to_ru() {
        // en_US knows «ye»; on RU layout that key chord is «ну».
        assert_eq!(convert_en_to_ru("ye"), "ну");
        assert_eq!(convert_en_to_ru("jr"), "ок");
        assert_eq!(convert_en_to_ru("j,"), "об"); // comma key = б — not ок
        assert!(field_has_cyrillic("ye теперь то все jr вроде!"));
    }

    #[test]
    fn layout_letters_include_ru_key_punct() {
        // `,sdftn` = бывает (будет would be `,eltn`)
        assert_eq!(convert_en_to_ru(",sdftn"), "бывает");
        assert_eq!(convert_en_to_ru(",eltn"), "будет");
        assert_eq!(convert_en_to_ru("[kt,"), "хлеб");
        assert_eq!(convert_en_to_ru("f"), "а");
        let words = latin_words_in("f xnj tckb ye jr ,sdftn njulf [kt, dsrbye");
        let toks: Vec<&str> = words.iter().map(|(_, _, w)| *w).collect();
        assert_eq!(
            toks,
            vec!["f", "xnj", "tckb", "ye", "jr", ",sdftn", "njulf", "[kt,", "dsrbye"]
        );
    }

    #[test]
    fn bare_period_comma_not_layout_tokens() {
        // After Cyrillic, real `.` / `,` must stay punctuation (not ю / б).
        let words = latin_words_in("ок. да, нет!");
        assert!(words.is_empty(), "got {words:?}");
        assert!(latin_words_in(".").is_empty());
        assert!(latin_words_in(",").is_empty());
        assert!(latin_words_in("привет...").is_empty());
        // Punct keys still glue to Latin letters (`,sdftn`, trailing `,` on a word).
        let glued = latin_words_in(",sdftn ok,");
        let toks: Vec<&str> = glued.iter().map(|(_, _, w)| *w).collect();
        assert_eq!(toks, vec![",sdftn", "ok,"]);
    }

    #[test]
    fn all_caps_abbrev_detected() {
        assert!(is_all_caps_abbrev("PDF"));
        assert!(is_all_caps_abbrev("OK"));
        assert!(!is_all_caps_abbrev("Ok"));
        assert!(!is_all_caps_abbrev("pdf"));
        assert!(!is_all_caps_abbrev("f"));
    }

    #[test]
    fn flip_by_script_never_ru_to_en() {
        // Pure Cyrillic must not become Latin («а»↛«ф»).
        assert_eq!(flip_by_script("хмм а что"), None);
        assert_eq!(flip_by_script("а"), None);
    }

    #[test]
    fn url_tokens_not_layout() {
        assert!(looks_like_url_token("https://rive.app"));
        assert!(looks_like_url_token("rive.app"));
        assert!(looks_like_url_token("example.com/path"));
        assert!(looks_like_url_token("www.google.com"));
        assert!(looks_like_url_token("mote.git"));
        assert!(!looks_like_url_token("rfr"));
        assert!(!looks_like_url_token("ghbdtn"));
        assert!(!looks_like_url_token("dhjlt"));
        // Whole field = URL only → skip. Mixed prose + URL → allow (token skip).
        assert!(looks_like_url_or_code("https://rive.app/docs"));
        assert!(looks_like_url_or_code(
            "https://github.com/alekseichmsk/mote.git"
        ));
        assert!(looks_like_url_or_code("git@github.com:alekseichmsk/mote.git"));
        assert!(!looks_like_url_or_code("привет rfr "));
        assert!(!looks_like_url_or_code(
            "сейчас попробую https://rive.app/ dhjlt dct jr "
        ));
        assert!(should_skip_latin_token_url_only("https://rive.app/"));
    }

    #[test]
    fn github_url_path_tokens_in_span() {
        let url = "https://github.com/alekseichmsk/mote.git";
        let spans = url_spans(url);
        assert_eq!(spans, vec![(0, url.len())]);
        let words = latin_words_in(url);
        let toks: Vec<&str> = words.iter().map(|(_, _, w)| *w).collect();
        assert!(toks.iter().any(|t| *t == "alekseichmsk") || toks.iter().any(|t| t.contains("alekseichmsk")));
        for (s, e, w) in &words {
            assert!(
                spans.iter().any(|&(a, b)| ranges_overlap(*s, *e, a, b)),
                "token «{w}» [{s},{e}) must sit in url span"
            );
        }
        // Prose + link: only the link span is protected.
        let mixed = "смотри https://github.com/alekseichmsk/mote.git и ghbdtn ";
        let spans = url_spans(mixed);
        assert_eq!(spans.len(), 1);
        let (us, ue) = spans[0];
        assert!(mixed[us..ue].contains("github.com"));
        let ghb = latin_words_in(mixed)
            .into_iter()
            .find(|(_, _, w)| *w == "ghbdtn")
            .expect("ghbdtn token");
        assert!(!ranges_overlap(ghb.0, ghb.1, us, ue));
    }

    fn should_skip_latin_token_url_only(word: &str) -> bool {
        looks_like_url_token(word)
    }
}
