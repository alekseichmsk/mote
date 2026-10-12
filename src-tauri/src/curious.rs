//! Free no-key APIs for short Mote bubble lines (Wikipedia, books, countries, quotes, NASA, numbers).

use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const UA: &str = "Mote/0.1 (+desktop companion; +https://github.com/)";
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CHARS: usize = 280;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CuriousLine {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

struct MemCache {
    key: String,
    at: Instant,
    line: CuriousLine,
}

static CACHE: Mutex<Vec<MemCache>> = Mutex::new(Vec::new());

fn cache_get(key: &str, ttl: Duration) -> Option<CuriousLine> {
    let guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .iter()
        .find(|c| c.key == key && c.at.elapsed() < ttl)
        .map(|c| c.line.clone())
}

fn cache_put(key: String, line: CuriousLine) {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|c| c.key != key);
    guard.push(MemCache {
        key,
        at: Instant::now(),
        line,
    });
    if guard.len() > 48 {
        guard.remove(0);
    }
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if out.chars().count() >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn http_get_text(url: &str) -> Result<String, String> {
    ureq::get(url)
        .set("User-Agent", UA)
        .set("Accept", "*/*")
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

fn http_get_json(url: &str) -> Result<Value, String> {
    let body = http_get_text(url)?;
    serde_json::from_str(&body).map_err(|e| format!("json: {e}"))
}

fn http_post_json(url: &str, body: &str) -> Result<Value, String> {
    let text = ureq::post(url)
        .set("User-Agent", UA)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .timeout(HTTP_TIMEOUT)
        .send_string(body)
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("json: {e}"))
}

fn day_key() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// Trivia / math fact about a number (Numbers API, wiki fallback).
pub fn number_fact(n: u64, kind: Option<String>) -> Result<CuriousLine, String> {
    let kind = match kind.as_deref().unwrap_or("trivia") {
        "math" => "math",
        "year" => "year",
        _ => "trivia",
    };
    let key = format!("num:{kind}:{n}");
    if let Some(hit) = cache_get(&key, Duration::from_secs(6 * 3600)) {
        return Ok(hit);
    }
    let url = format!("http://numbersapi.com/{n}/{kind}");
    if let Ok(raw) = http_get_text(&url) {
        let text = truncate(raw.trim(), MAX_CHARS);
        if !text.is_empty() && !text.contains("<html") {
            let line = CuriousLine {
                text: format!("Кстати: {text}"),
                url: Some(url),
            };
            cache_put(key, line.clone());
            return Ok(line);
        }
    }
    // Numbers API is often down — fall back to Wikipedia about the number.
    let mut line = wiki_summary(n.to_string())?;
    if !line.text.starts_with("Кстати:") {
        line.text = truncate(&format!("Кстати: {}", line.text), MAX_CHARS);
    }
    cache_put(key, line.clone());
    Ok(line)
}

/// Fact about a calendar date (month/day).
pub fn date_fact(month: u32, day: u32) -> Result<CuriousLine, String> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err("нужна дата месяц/день".into());
    }
    let key = format!("date:{month}:{day}");
    if let Some(hit) = cache_get(&key, Duration::from_secs(24 * 3600)) {
        return Ok(hit);
    }
    let url = format!("http://numbersapi.com/{month}/{day}/date");
    if let Ok(raw) = http_get_text(&url) {
        let text = truncate(raw.trim(), MAX_CHARS);
        if !text.is_empty() && !text.contains("<html") {
            let line = CuriousLine {
                text,
                url: Some(url),
            };
            cache_put(key, line.clone());
            return Ok(line);
        }
    }
    let months = [
        "",
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    let q = format!("{day} {}", months[month as usize]);
    let line = wiki_summary(q)?;
    cache_put(key, line.clone());
    Ok(line)
}

/// Russian quotes offline (no EN DummyJSON). One pick per day via cache.
const RU_QUOTES: &[(&str, &str)] = &[
    ("Счастье есть, его не может не быть.", "Антон Чехов"),
    ("Все счастливые семьи похожи друг на друга.", "Лев Толстой"),
    ("Красота спасёт мир.", "Фёдор Достоевский"),
    ("Горе от ума.", "Александр Грибоедов"),
    ("Я вас любил: любовь ещё, быть может…", "Александр Пушкин"),
    ("Умом Россию не понять.", "Фёдор Тютчев"),
    (
        "Быть можно дельным человеком и думать о красе ногтей.",
        "Александр Пушкин",
    ),
    ("Человек — это звучит гордо.", "Максим Горький"),
    ("Жизнь прожить — не поле перейти.", "народная"),
    ("Тише едешь — дальше будешь.", "народная"),
    ("Не в силе Бог, а в правде.", "Александр Невский"),
    ("Лучше горькая правда, чем сладкая ложь.", "народная"),
    ("Делу время, потехе час.", "народная"),
    ("Что имеем — не храним, потерявши — плачем.", "народная"),
    ("Век живи — век учись.", "народная"),
    ("Семь раз отмерь, один раз отрежь.", "народная"),
    ("Без труда не вытащишь и рыбку из пруда.", "народная"),
    ("Не место красит человека, а человек место.", "народная"),
    ("Друзья познаются в беде.", "народная"),
    ("Кто рано встаёт, тому Бог подаёт.", "народная"),
    ("Я мыслю, следовательно, существую.", "Рене Декарт"),
    ("Знание — сила.", "Фрэнсис Бэкон"),
    ("Дорогу осилит идущий.", "народная"),
    ("Начало — половина всего.", "народная"),
];

/// Day quote (stable) or command «цитата» — always Russian, offline.
pub fn random_quote() -> Result<CuriousLine, String> {
    let key = format!("quote:day:{}", day_key());
    if let Some(hit) = cache_get(&key, Duration::from_secs(20 * 3600)) {
        return Ok(hit);
    }
    if RU_QUOTES.is_empty() {
        return Err("цитаты пусты".into());
    }
    let idx = (day_key() as usize).wrapping_mul(2654435761) % RU_QUOTES.len();
    let (content, author) = RU_QUOTES[idx];
    let text = truncate(&format!("«{content}» — {author}"), MAX_CHARS);
    let line = CuriousLine { text, url: None };
    cache_put(key, line.clone());
    Ok(line)
}

fn wiki_summary_url(title_or_path: &str) -> String {
    format!(
        "https://ru.wikipedia.org/api/rest_v1/page/summary/{}",
        encode_path(title_or_path.trim())
    )
}

/// Pull «род. 7 октября 1952» (etc.) to the front for birth-style questions.
fn birth_prefix(extract: &str) -> Option<String> {
    let lower = extract.to_lowercase();
    for marker in ["род.", "родился ", "родилась ", "родились "] {
        let Some(pos) = lower.find(marker) else {
            continue;
        };
        let rest = extract.get(pos..)?;
        let mut out = String::new();
        let mut seen_digit = false;
        for ch in rest.chars() {
            out.push(ch);
            if ch.is_ascii_digit() {
                seen_digit = true;
            }
            if ch == '.' && seen_digit && out.chars().count() > 12 {
                break;
            }
            if out.chars().count() > 52 {
                break;
            }
        }
        let bit = out.trim();
        if seen_digit && bit.chars().count() >= 8 {
            return Some(bit.to_string());
        }
    }
    None
}

fn wiki_from_json(v: &Value) -> Result<CuriousLine, String> {
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .unwrap_or("Статья")
        .trim();
    let extract = v
        .get("extract")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim();
    if extract.is_empty() {
        return Err("нет краткого текста".into());
    }
    let page_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    if page_type == "disambiguation" {
        return Err("слишком общее имя — уточни запрос".into());
    }
    let page_url = v
        .pointer("/content_urls/desktop/page")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            Some(format!(
                "https://ru.wikipedia.org/wiki/{}",
                encode_path(title)
            ))
        });
    let text = if let Some(born) = birth_prefix(extract) {
        truncate(&format!("{title}: {born}"), MAX_CHARS)
    } else {
        truncate(&format!("{title}: {extract}"), MAX_CHARS)
    };
    Ok(CuriousLine {
        text,
        url: page_url,
    })
}

/// Title suggest — good for short names («путин»), weak for full questions.
fn wiki_opensearch(q: &str) -> Result<String, String> {
    let search = format!(
        "https://ru.wikipedia.org/w/api.php?action=opensearch&search={}&limit=1&namespace=0&format=json",
        encode_path(q)
    );
    let arr = http_get_json(&search)?;
    arr.get(1)
        .and_then(|a| a.get(0))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "opensearch пусто".to_string())
}

/// Full-text search — handles «из чего состоит пластик» → «Пластмасса».
fn wiki_fulltext_search(q: &str) -> Result<String, String> {
    let search = format!(
        "https://ru.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&srlimit=1&srnamespace=0&format=json",
        encode_path(q)
    );
    let v = http_get_json(&search)?;
    v.pointer("/query/search/0/title")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "ничего не нашёл в Википедии".to_string())
}

fn wiki_search_title(q: &str) -> Result<String, String> {
    let q = q.trim().trim_end_matches(['?', '!', '.', '…', '¿', '¡']);
    let q = q.trim();
    if q.is_empty() {
        return Err("пустой запрос".into());
    }
    // Short tokens: prefer opensearch. Longer / question-like: fulltext first.
    let words = q.split_whitespace().count();
    if words <= 2 {
        wiki_opensearch(q).or_else(|_| wiki_fulltext_search(q))
    } else {
        wiki_fulltext_search(q).or_else(|_| wiki_opensearch(q))
    }
}

pub fn wiki_summary(query: String) -> Result<CuriousLine, String> {
    let q = query
        .trim()
        .trim_end_matches(['?', '!', '.', '…', '¿', '¡'])
        .trim();
    if q.is_empty() {
        return Err("пустой запрос".into());
    }
    let key = format!("wiki:{}", q.to_lowercase());
    if let Some(hit) = cache_get(&key, Duration::from_secs(6 * 3600)) {
        return Ok(hit);
    }
    // Search first — questions and short names both resolve to a real article title.
    let title = wiki_search_title(q)?;
    let v = http_get_json(&wiki_summary_url(&title))
        .or_else(|_| http_get_json(&wiki_summary_url(q)))
        .map_err(|e| format!("Википедия: {e}"))?;
    let line = wiki_from_json(&v)?;
    cache_put(key, line.clone());
    Ok(line)
}

pub fn wiki_random() -> Result<CuriousLine, String> {
    let key = format!("wiki-rand:{}", day_key() % 17);
    if let Some(hit) = cache_get(&key, Duration::from_secs(30 * 60)) {
        return Ok(hit);
    }
    let url = "https://ru.wikipedia.org/api/rest_v1/page/random/summary";
    let v = http_get_json(url)?;
    let line = wiki_from_json(&v)?;
    cache_put(key, line.clone());
    Ok(line)
}

pub fn country_info(name: String) -> Result<CuriousLine, String> {
    let q = name.trim();
    if q.is_empty() {
        return Err("какая страна?".into());
    }
    let key = format!("country:{}", q.to_lowercase());
    if let Some(hit) = cache_get(&key, Duration::from_secs(24 * 3600)) {
        return Ok(hit);
    }
    // Latin names: CountriesNow. Cyrillic / anything else: Wikipedia (RU).
    let asciiish = q.chars().all(|c| c.is_ascii());
    if asciiish {
        let body = format!(r#"{{"country":"{}"}}"#, q.replace('"', ""));
        if let Ok(v) = http_post_json(
            "https://countriesnow.space/api/v0.1/countries/capital",
            &body,
        ) {
            let err = v.get("error").and_then(|x| x.as_bool()).unwrap_or(true);
            if !err {
                let common = v
                    .pointer("/data/name")
                    .and_then(|x| x.as_str())
                    .unwrap_or(q);
                let capital = v
                    .pointer("/data/capital")
                    .and_then(|x| x.as_str())
                    .unwrap_or("—");
                let line = CuriousLine {
                    text: truncate(&format!("{common}: столица {capital}"), MAX_CHARS),
                    url: Some(format!(
                        "https://ru.wikipedia.org/wiki/{}",
                        encode_path(common)
                    )),
                };
                cache_put(key, line.clone());
                return Ok(line);
            }
        }
    }
    let line = wiki_summary(q.to_string())?;
    cache_put(key, line.clone());
    Ok(line)
}

pub fn book_search(query: String) -> Result<CuriousLine, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("какая книга?".into());
    }
    let key = format!("book:{}", q.to_lowercase());
    if let Some(hit) = cache_get(&key, Duration::from_secs(6 * 3600)) {
        return Ok(hit);
    }
    let url = format!(
        "https://openlibrary.org/search.json?q={}&limit=1",
        encode_path(q)
    );
    let v = http_get_json(&url)?;
    let doc = v
        .get("docs")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "книгу не нашёл".to_string())?;
    let title = doc
        .get("title")
        .and_then(|x| x.as_str())
        .unwrap_or("Без названия");
    let author = doc
        .get("author_name")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|x| x.as_str())
        .unwrap_or("неизвестный автор");
    let year = doc
        .get("first_publish_year")
        .and_then(|x| x.as_u64())
        .map(|y| format!(", {y}"))
        .unwrap_or_default();
    let key_path = doc
        .get("key")
        .and_then(|x| x.as_str())
        .map(|k| format!("https://openlibrary.org{k}"));
    let text = truncate(&format!("Книга: {title} — {author}{year}"), MAX_CHARS);
    let line = CuriousLine {
        text,
        url: key_path,
    };
    cache_put(key, line.clone());
    Ok(line)
}

pub fn nasa_apod() -> Result<CuriousLine, String> {
    let key = format!("apod:{}", day_key());
    if let Some(hit) = cache_get(&key, Duration::from_secs(20 * 3600)) {
        return Ok(hit);
    }
    let url = "https://api.nasa.gov/planetary/apod?api_key=DEMO_KEY";
    let v = http_get_json(url)?;
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .unwrap_or("APOD")
        .trim();
    let explanation = v
        .get("explanation")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim();
    let page = v
        .get("hdurl")
        .or_else(|| v.get("url"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let blurb = if explanation.is_empty() {
        title.to_string()
    } else {
        let first = explanation.split(". ").next().unwrap_or(explanation);
        format!("{title}: {first}.")
    };
    let line = CuriousLine {
        text: truncate(&format!("NASA сегодня: {blurb}"), MAX_CHARS),
        url: page,
    };
    cache_put(key, line.clone());
    Ok(line)
}

/// Open a URL in the default browser (Windows).
pub fn open_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("только http(s)".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err("открытие ссылок только на Windows".into())
    }
}
