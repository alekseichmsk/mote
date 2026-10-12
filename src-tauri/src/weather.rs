//! Short weather line via https://wttr.in (no API key).

use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Cache {
    city_key: String,
    at: Instant,
    line: String,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
const TTL: Duration = Duration::from_secs(20 * 60);

fn normalize_city(city: &str) -> String {
    let t = city.trim();
    if t.is_empty() {
        "Moscow".into()
    } else {
        t.to_string()
    }
}

fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// One-line forecast, e.g. `Moscow: ⛅ +12°C · ощущается +9°C`.
pub fn weather_now(city: String) -> Result<String, String> {
    let city = normalize_city(&city);
    let key = format!("v2:{}", city.to_lowercase());

    {
        let guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = guard.as_ref() {
            if c.city_key == key && c.at.elapsed() < TTL && !c.line.is_empty() {
                return Ok(c.line.clone());
            }
        }
    }

    let path = encode_path_segment(&city);
    // %t = temperature, %f = "feels like" — join with a marker we can rewrite.
    let url = format!("https://wttr.in/{path}?format=%l:+%c+%t|%f&lang=ru");

    let body = ureq::get(&url)
        .set("User-Agent", "Mote/0.1 (+desktop companion)")
        .set("Accept-Language", "ru")
        .timeout(Duration::from_secs(8))
        .call()
        .map_err(|e| format!("wttr.in: {e}"))?
        .into_string()
        .map_err(|e| format!("wttr.in read: {e}"))?;

    let raw = body.trim().trim_matches('"').to_string();
    if raw.is_empty() || raw.contains("Unknown location") {
        return Err(format!("не нашёл погоду для «{city}»"));
    }

    let line = match raw.split_once('|') {
        Some((main, feels)) => {
            let main = main.trim();
            let feels = feels.trim();
            let temp = main.rsplit(' ').next().unwrap_or("");
            if feels.is_empty() || feels == temp {
                main.to_string()
            } else {
                format!("{main}\nощущается {feels}")
            }
        }
        None => raw,
    };

    {
        let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(Cache {
            city_key: key,
            at: Instant::now(),
            line: line.clone(),
        });
    }

    eprintln!("[mote] weather: {line}");
    Ok(line)
}
