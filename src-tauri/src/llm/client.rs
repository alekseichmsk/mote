//! OpenAI-compatible client against local llama-server.

use super::sidecar;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

const SYSTEM: &str = "Исправь орфографию, пунктуацию и явные ошибки в тексте пользователя. \
Сохрани смысл и язык. Верни ТОЛЬКО исправленный текст без кавычек, без markdown и без пояснений.";

pub fn improve(text: &str) -> Result<String, String> {
    let port = sidecar::current_port();
    if port == 0 {
        return Err("sidecar не запущен".into());
    }
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let body = json!({
        "model": "local",
        "temperature": 0.0,
        "max_tokens": 2048,
        "messages": [
            { "role": "system", "content": SYSTEM },
            { "role": "user", "content": text },
        ],
    });
    let resp = ureq::post(&url)
        .timeout(Duration::from_secs(120))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("llm http: {e}"))?;
    let parsed: ChatResponse = resp.into_json().map_err(|e| format!("llm json: {e}"))?;
    let raw = parsed
        .choices
        .first()
        .and_then(|c| c.message.content.as_ref())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    Ok(strip_wrappers(&raw))
}

fn strip_wrappers(s: &str) -> String {
    let mut t = s.trim().to_string();
    if t.starts_with("```") {
        if let Some(rest) = t.strip_prefix("```") {
            let rest = rest
                .strip_prefix("text")
                .or_else(|| rest.strip_prefix("txt"))
                .unwrap_or(rest)
                .trim_start_matches('\n');
            if let Some(end) = rest.rfind("```") {
                t = rest[..end].trim().to_string();
            } else {
                t = rest.trim().to_string();
            }
        }
    }
    let quote_chars: &[char] = &['"', '\'', '«', '»'];
    while t.chars().count() >= 2 {
        let mut chars = t.chars();
        let first = chars.next();
        let last = t.chars().last();
        match (first, last) {
            (Some(a), Some(b))
                if quote_chars.contains(&a)
                    && (quote_chars.contains(&b) || (a == '«' && b == '»')) =>
            {
                t = t
                    .chars()
                    .skip(1)
                    .take(t.chars().count().saturating_sub(2))
                    .collect();
                t = t.trim().to_string();
            }
            _ => break,
        }
    }
    t
}
