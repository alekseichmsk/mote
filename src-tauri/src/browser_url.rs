//! Read the active browser tab URL via UI Automation (address bar).
//! Used to detect video pages when the window title is ambiguous (Pinterest, etc.).

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Cache {
    hwnd: isize,
    url: String,
    at: Instant,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
const CACHE_TTL: Duration = Duration::from_millis(1200);

/// URL / host fragments that mean "there's a video page open".
const VIDEO_URL_MARKERS: &[&str] = &[
    "youtube.com/watch",
    "youtube.com/shorts",
    "youtube.com/live",
    "youtu.be/",
    "netflix.com/watch",
    "twitch.tv/",
    "rutube.ru/video",
    "rutube.ru/play",
    "kinopoisk.ru/film",
    "kinopoisk.ru/series",
    "kinopoisk.ru/watch",
    "disneyplus.com/",
    "primevideo.com/",
    "amazon.com/gp/video",
    "vk.com/video",
    "vk.ru/video",
    "vimeo.com/",
    "ivi.ru/watch",
    "okko.tv/",
    "wink.ru/",
    "premier.one/",
    "tiktok.com/@",
    "tiktok.com/t/",
    "vm.tiktok.com/",
    "instagram.com/reel",
    "instagram.com/reels",
    "instagram.com/tv/",
    // Pin pages often host video; title alone is unreliable.
    "pinterest.com/pin/",
    "pinterest.ru/pin/",
    "pin.it/",
];

pub fn url_looks_like_video(url: &str) -> bool {
    let u = normalize_url(url);
    if u.is_empty() {
        return false;
    }
    VIDEO_URL_MARKERS.iter().any(|m| u.contains(m))
}

fn normalize_url(url: &str) -> String {
    let t = url.trim().to_lowercase();
    if t.is_empty() || t.contains(' ') && !t.contains('.') {
        // Omnibox search text, not a URL.
        return String::new();
    }
    if t.starts_with("http://") || t.starts_with("https://") {
        t
    } else if t.contains('.') {
        format!("https://{t}")
    } else {
        String::new()
    }
}

/// Cached address-bar URL for the given top-level browser HWND.
pub fn active_tab_url(hwnd: isize, pid: u32) -> Option<String> {
    if hwnd == 0 || pid == 0 {
        return None;
    }

    {
        let guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = guard.as_ref() {
            if c.hwnd == hwnd && c.at.elapsed() < CACHE_TTL {
                return if c.url.is_empty() {
                    None
                } else {
                    Some(c.url.clone())
                };
            }
        }
    }

    let url = read_url_uia(pid).unwrap_or_default();
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(Cache {
        hwnd,
        url: url.clone(),
        at: Instant::now(),
    });
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

#[cfg(windows)]
fn read_url_uia(pid: u32) -> Option<String> {
    use uiautomation::types::{ControlType, UIProperty};
    use uiautomation::UIAutomation;

    let automation = UIAutomation::new().ok()?;
    let root = automation.get_root_element().ok()?;
    let browser = automation
        .create_matcher()
        .from(root)
        .timeout(400)
        .process_id(pid)
        .find_first()
        .ok()?;

    // Chromium family: ToolbarView → LocationBarView → Edit (omnibox).
    if let Ok(toolbar) = automation
        .create_matcher()
        .from(browser.clone())
        .timeout(250)
        .classname("ToolbarView")
        .find_first()
    {
        if let Ok(location) = automation
            .create_matcher()
            .from(toolbar)
            .timeout(200)
            .classname("LocationBarView")
            .find_first()
        {
            if let Ok(edit) = automation
                .create_matcher()
                .from(location)
                .timeout(200)
                .control_type(ControlType::Edit)
                .find_first()
            {
                if let Ok(val) = edit.get_property_value(UIProperty::ValueValue) {
                    if let Ok(s) = val.get_string() {
                        let s = s.trim().to_string();
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                }
            }
        }
    }

    // Firefox / fallback: first Edit under the browser chrome (not page content).
    // Prefer name hints used by Firefox urlbar.
    if let Ok(edit) = automation
        .create_matcher()
        .from(browser)
        .timeout(350)
        .control_type(ControlType::Edit)
        .find_first()
    {
        if let Ok(val) = edit.get_property_value(UIProperty::ValueValue) {
            if let Ok(s) = val.get_string() {
                let s = s.trim().to_string();
                if !s.is_empty() && (s.contains('.') || s.contains('/')) {
                    return Some(s);
                }
            }
        }
    }

    None
}

#[cfg(not(windows))]
fn read_url_uia(_pid: u32) -> Option<String> {
    None
}
