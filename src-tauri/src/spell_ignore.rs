//! Personal ignore list for spell / layout_auto (`хмм`, abbr). `%APPDATA%\Mote\spell_ignore.json`.

use std::collections::HashSet;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

static IGNORE: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Bumped on every successful add — watcher clears a stale plaque.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Default, Serialize, Deserialize)]
struct IgnoreFile {
    #[serde(default)]
    words: Vec<String>,
}

fn path() -> std::path::PathBuf {
    crate::llm::mote_data_dir().join("spell_ignore.json")
}

fn normalize(word: &str) -> String {
    word.trim().chars().flat_map(|c| c.to_lowercase()).collect()
}

fn ensure_loaded(guard: &mut Option<HashSet<String>>) {
    if guard.is_none() {
        let set = load_from_disk();
        eprintln!("[mote] spell_ignore loaded n={}", set.len());
        *guard = Some(set);
    }
}

fn load_from_disk() -> HashSet<String> {
    let Ok(raw) = fs::read_to_string(path()) else {
        return HashSet::new();
    };
    match serde_json::from_str::<IgnoreFile>(&raw) {
        Ok(file) => file
            .words
            .into_iter()
            .map(|w| normalize(&w))
            .filter(|w| !w.is_empty())
            .collect(),
        Err(e) => {
            eprintln!("[mote] spell_ignore parse err: {e}");
            HashSet::new()
        }
    }
}

fn save(set: &HashSet<String>) -> Result<(), String> {
    let dir = crate::llm::mote_data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("spell_ignore dir: {e}"))?;
    let mut words: Vec<String> = set.iter().cloned().collect();
    words.sort();
    let raw = serde_json::to_string_pretty(&IgnoreFile { words }).map_err(|e| e.to_string())?;
    let tmp = dir.join("spell_ignore.json.tmp");
    fs::write(&tmp, &raw).map_err(|e| format!("spell_ignore tmp: {e}"))?;
    fs::rename(&tmp, path()).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("spell_ignore rename: {e}")
    })
}

pub fn init() {
    let Ok(mut guard) = IGNORE.lock() else {
        return;
    };
    ensure_loaded(&mut guard);
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

pub fn is_ignored(word: &str) -> bool {
    let key = normalize(word);
    if key.is_empty() {
        return false;
    }
    let Ok(mut guard) = IGNORE.lock() else {
        return false;
    };
    ensure_loaded(&mut guard);
    match guard.as_ref() {
        Some(set) => set.contains(&key),
        None => false,
    }
}

/// True when every typo word in a plaque offer is on the ignore list (Keep raced).
pub fn offer_fully_ignored(word_label: &str, fix_words: &[String]) -> bool {
    if !fix_words.is_empty() {
        return fix_words.iter().all(|w| is_ignored(w));
    }
    let parts: Vec<&str> = word_label
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    !parts.is_empty() && parts.iter().all(|w| is_ignored(w))
}

/// Add one or more words (comma-separated plaque labels ok).
pub fn add_words(raw: &str) -> Result<usize, String> {
    let parts: Vec<String> = raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(normalize)
        .filter(|w| w.chars().count() >= 2)
        .collect();
    if parts.is_empty() {
        return Err("нечего запоминать".into());
    }
    let Ok(mut guard) = IGNORE.lock() else {
        return Err("spell_ignore lock".into());
    };
    ensure_loaded(&mut guard);
    let mut next = match guard.as_ref() {
        Some(set) => set.clone(),
        None => HashSet::new(),
    };
    let mut n = 0usize;
    for w in parts {
        if next.insert(w.clone()) {
            n += 1;
            eprintln!("[mote] spell_ignore add «{w}»");
        }
    }
    if n == 0 {
        return Ok(0);
    }
    save(&next)?;
    *guard = Some(next);
    GENERATION.fetch_add(1, Ordering::Relaxed);
    Ok(n)
}
