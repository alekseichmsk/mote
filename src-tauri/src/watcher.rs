//! Windows presence: cursor eyes, dock beside video / clicks / maximize,
//! body + eyes beside caret while typing.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

static PRESENCE_ENABLED: AtomicBool = AtomicBool::new(true);
static PRESENCE_PAUSED: AtomicBool = AtomicBool::new(false);
/// Win32 GetTickCount deadline вЂ” while set, don't auto-move after manual drag.
static USER_PLACED_UNTIL: AtomicU64 = AtomicU64::new(0);
/// Shoo / explicit dismiss: drop caret compose lock even while still in a text field.
static CLEAR_INPUT_LOCK: AtomicBool = AtomicBool::new(false);
/// Edge tracker for dismissing the side menu on outside click.
static MENU_LMB_DOWN: AtomicBool = AtomicBool::new(false);
/// Full-screen tantrum after too many shoos — blocks docking + covers work area.
static RAGE_MODE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct SavedGeom {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

static RAGE_SAVED: Mutex<Option<SavedGeom>> = Mutex::new(None);

pub fn set_enabled(enabled: bool) {
    PRESENCE_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn set_paused(paused: bool) {
    // Don't unpause while the tantrum overlay owns the screen.
    if !paused && RAGE_MODE.load(Ordering::Relaxed) {
        return;
    }
    PRESENCE_PAUSED.store(paused, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    PRESENCE_ENABLED.load(Ordering::Relaxed)
}

pub fn is_rage_mode() -> bool {
    RAGE_MODE.load(Ordering::Relaxed)
}

pub fn mark_user_placed(duration_ms: u64) {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetTickCount;
        let now = unsafe { GetTickCount() } as u64;
        USER_PLACED_UNTIL.store(now.saturating_add(duration_ms), Ordering::Relaxed);
    }
    #[cfg(not(windows))]
    {
        let _ = duration_ms;
    }
}

/// Drop caret/typing dock (used by shoo so glasses mode can't yank Mote back).
pub fn clear_input_lock() {
    CLEAR_INPUT_LOCK.store(true, Ordering::Relaxed);
}

fn is_user_placed() -> bool {
    let until = USER_PLACED_UNTIL.load(Ordering::Relaxed);
    if until == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetTickCount;
        (unsafe { GetTickCount() } as u64) < until
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookPayload {
    pub dir: String,
    pub mode: String,
    /// Continuous look aim in roughly -1вЂ¦1 (toward target).
    pub nx: f32,
    pub ny: f32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivatePayload {
    pub reason: String,
    pub video: bool,
}

/// Digits the user just typed (any focused app). Empty = dismiss offer.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DigitOfferPayload {
    pub digits: String,
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_BACK, VK_CONTROL, VK_DELETE, VK_LBUTTON, VK_MENU, VK_RETURN, VK_SHIFT,
        VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsZoomed, GUITHREADINFO,
    };

    /// Strict title markers for browser tabs (avoid random sites).
    const BROWSER_VIDEO_MARKERS: &[&str] = &[
        " - youtube",
        " | youtube",
        "youtube -",
        "(youtube)",
        " - netflix",
        " | netflix",
        "netflix ",
        " - twitch",
        " | twitch",
        "twitch -",
        " - vimeo",
        // Rutube titles vary: "вЂ¦ вЂ” RUTUBE", "| RUTUBE", "РЅР° RUTUBE", "rutube.ru"
        "rutube",
        "СЂСѓС‚СѓР±",
        " - kinopoisk",
        " | kinopoisk",
        "kinopoisk.ru",
        " - РєРёРЅРѕРїРѕРёСЃРє",
        " | РєРёРЅРѕРїРѕРёСЃРє",
        "РєРёРЅРѕРїРѕРёСЃРє ",
        "РєРёРЅРѕРїРѕРёСЃРє.ru",
        "prime video",
        "amazon prime video",
        "disney+",
        "disney plus",
        "hbo max",
        "vk.com/video",
        "vk video",
        "premier.one",
        "ivi.ru",
        "okko.tv",
        "wink.ru",
        "more.tv",
        // Pinterest / short-form: SMTC often reports these as Music, so title marks video.
        "pinterest",
        "РїРёРЅС‚РµСЂРµСЃС‚",
        "tiktok",
        "С‚РёРєС‚РѕРє",
        " - tik tok",
        "| tiktok",
    ];

    /// Non-browser apps / dedicated players (title contains).
    const APP_VIDEO_MARKERS: &[&str] = &[
        "movies & tv",
        "films & tv",
        "jellyfin",
        "plex media",
        " - plex",
        "potplayer",
        "mpc-hc",
        "mpc-be",
        "vlc media player",
    ];

    const BROWSER_EXES: &[&str] = &[
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "brave.exe",
        "opera.exe",
        "opera_crashreporter.exe",
        "vivaldi.exe",
        "browser.exe", // Yandex
        "yandex.exe",
        "arc.exe",
        "chromium.exe",
    ];

    /// Known local player executables (basename, lowercase).
    const VIDEO_PLAYER_EXES: &[&str] = &[
        "vlc.exe",
        "mpv.exe",
        "mpvnet.exe",
        "potplayer.exe",
        "potplayermini64.exe",
        "potplayermini.exe",
        "mpc-hc64.exe",
        "mpc-hc.exe",
        "mpc-be64.exe",
        "mpc-be.exe",
        "wmplayer.exe",
        "film&tv.exe",
        "video.ui.exe",
        "plex.exe",
        "jellyfinmediaplayer.exe",
    ];

    fn cursor_pos() -> Option<(i32, i32)> {
        let mut pt = POINT::default();
        unsafe {
            GetCursorPos(&mut pt).ok()?;
        }
        Some((pt.x, pt.y))
    }

    fn foreground_hwnd() -> HWND {
        unsafe { GetForegroundWindow() }
    }

    fn window_title(hwnd: HWND) -> String {
        let mut buf = [0u16; 512];
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }

    fn foreground_info() -> Option<(HWND, RECT, String, String)> {
        let hwnd = foreground_hwnd();
        if hwnd.0.is_null() {
            return None;
        }
        let mut rc = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut rc).ok()?;
        }
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        if w < 260 || h < 220 {
            return None;
        }
        let title = window_title(hwnd);
        let exe = process_exe_name(hwnd);
        Some((hwnd, rc, title, exe))
    }

    fn process_exe_name(hwnd: HWND) -> String {
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == 0 {
            return String::new();
        }
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return String::new();
            };
            let mut buf = [0u16; 260];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                Default::default(),
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);
            if ok.is_err() || size == 0 {
                return String::new();
            }
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()
        }
    }

    fn is_browser_exe(exe: &str) -> bool {
        let e = exe.to_lowercase();
        BROWSER_EXES.iter().any(|b| e == *b)
            || e.contains("chrome")
            || e.contains("firefox")
            || e.contains("msedge")
    }

    fn hwnd_pid(hwnd: HWND) -> u32 {
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        pid
    }

    fn is_video_window(title: &str, exe: &str, hwnd: HWND) -> bool {
        let exe = exe.to_lowercase();
        if VIDEO_PLAYER_EXES.iter().any(|p| exe == *p) {
            return true;
        }
        let t = title.to_lowercase();
        // Browsers: title markers first, then active-tab URL via UI Automation.
        if is_browser_exe(&exe) {
            if !t.is_empty() && BROWSER_VIDEO_MARKERS.iter().any(|k| t.contains(k)) {
                return true;
            }
            let pid = hwnd_pid(hwnd);
            if let Some(url) = crate::browser_url::active_tab_url(hwnd.0 as isize, pid) {
                return crate::browser_url::url_looks_like_video(&url);
            }
            return false;
        }
        if t.is_empty() {
            return false;
        }
        APP_VIDEO_MARKERS.iter().any(|k| t.contains(k))
            || (exe.contains("vlc") && !t.is_empty())
            || t.ends_with(" - vlc media player")
    }

    fn is_key_down(vk: i32) -> bool {
        unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
    }

    fn typing_keys_active() -> bool {
        if is_key_down(VK_SPACE.0 as i32)
            || is_key_down(VK_RETURN.0 as i32)
            || is_key_down(VK_BACK.0 as i32)
            || is_key_down(VK_DELETE.0 as i32)
        {
            return true;
        }
        // Letters, digits, numpad, OEM (RU layout punctuation).
        for vk in [0x30u16..=0x5A, 0x60..=0x69, 0xBA..=0xC0, 0xDB..=0xDF]
            .into_iter()
            .flatten()
        {
            if is_key_down(vk as i32) {
                return true;
            }
        }
        false
    }

    /// Trailing ASCII digit run in the focused field (what the user is editing).
    fn trailing_digit_run(text: &str) -> Option<String> {
        let s = text.trim_end();
        let run: String = s
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if run.is_empty() {
            None
        } else {
            Some(run)
        }
    }

    /// Edge-triggered digit / backspace while typing in any app.
    struct DigitTracker {
        row: [bool; 10],
        pad: [bool; 10],
        back: bool,
        delete: bool,
        /// Other typable keys that break a number run.
        breakers: [bool; 26],
        buf: String,
        dirty: bool,
        /// Last time Backspace/Delete trimmed or wiped the run (for empty-field sync).
        last_erase: Option<Instant>,
    }

    impl DigitTracker {
        fn new() -> Self {
            Self {
                row: [false; 10],
                pad: [false; 10],
                back: false,
                delete: false,
                breakers: [false; 26],
                buf: String::new(),
                dirty: false,
                last_erase: None,
            }
        }

        fn clear(&mut self) {
            if !self.buf.is_empty() {
                self.buf.clear();
                self.dirty = true;
            }
        }

        fn mark_erase(&mut self) {
            self.last_erase = Some(Instant::now());
        }

        fn erased_recently(&self) -> bool {
            self.last_erase
                .map(|t| t.elapsed() < Duration::from_millis(800))
                .unwrap_or(false)
        }

        /// Prefer real field contents over the keyboard run (select-all / paste / multi-delete).
        fn sync_from_field(&mut self, field_digits: &str) {
            let next: String = field_digits.chars().take(12).collect();
            if self.buf != next {
                eprintln!(
                    "[mote] digit sync {} → {}",
                    self.buf.chars().count(),
                    next.chars().count()
                );
                self.buf = next;
                self.dirty = true;
            }
        }

        fn poll(&mut self) {
            // Shift/Ctrl/Alt + digit → symbols/shortcuts (e.g. Shift+5 = %), not a number.
            let mods = is_key_down(VK_SHIFT.0 as i32)
                || is_key_down(VK_CONTROL.0 as i32)
                || is_key_down(VK_MENU.0 as i32);
            let ctrl = is_key_down(VK_CONTROL.0 as i32);

            // Top-row 0–9
            for i in 0..10 {
                let down = is_key_down(0x30 + i as i32);
                if down && !self.row[i] {
                    if mods {
                        // Symbol/shortcut — break any in-progress digit run.
                        self.clear();
                    } else if self.buf.len() < 12 {
                        self.buf.push(char::from(b'0' + i as u8));
                        self.dirty = true;
                    }
                }
                self.row[i] = down;
            }
            // Numpad 0–9 (Shift rarely remaps these to %; still ignore Ctrl/Alt shortcuts)
            let pad_mods = is_key_down(VK_CONTROL.0 as i32) || is_key_down(VK_MENU.0 as i32);
            for i in 0..10 {
                let down = is_key_down(0x60 + i as i32);
                if down && !self.pad[i] {
                    if pad_mods {
                        self.clear();
                    } else if self.buf.len() < 12 {
                        self.buf.push(char::from(b'0' + i as u8));
                        self.dirty = true;
                    }
                }
                self.pad[i] = down;
            }
            // Backspace / Delete: one char; Ctrl+Backspace/Delete = wipe whole word (Win32).
            let back = is_key_down(VK_BACK.0 as i32);
            if back && !self.back {
                if ctrl {
                    if !self.buf.is_empty() {
                        eprintln!("[mote] digit clear (Ctrl+Backspace)");
                        self.clear();
                        self.mark_erase();
                    }
                } else if !self.buf.is_empty() {
                    self.buf.pop();
                    self.dirty = true;
                    self.mark_erase();
                }
            }
            self.back = back;
            let del = is_key_down(VK_DELETE.0 as i32);
            if del && !self.delete {
                if ctrl {
                    if !self.buf.is_empty() {
                        eprintln!("[mote] digit clear (Ctrl+Delete)");
                        self.clear();
                        self.mark_erase();
                    }
                } else if !self.buf.is_empty() {
                    self.buf.pop();
                    self.dirty = true;
                    self.mark_erase();
                }
            }
            self.delete = del;

            // Letters A–Z end the number run (space/enter too).
            for i in 0..26 {
                let down = is_key_down(0x41 + i as i32);
                if down && !self.breakers[i] {
                    self.clear();
                }
                self.breakers[i] = down;
            }
            if is_key_down(VK_SPACE.0 as i32) || is_key_down(VK_RETURN.0 as i32) {
                // Keep buffer until letter/timeout; don't clear on space.
                let _ = ();
            }
        }

        fn take_dirty(&mut self) -> Option<String> {
            if self.dirty {
                self.dirty = false;
                Some(self.buf.clone())
            } else {
                None
            }
        }
    }

    fn gui_thread_info() -> Option<GUITHREADINFO> {
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            if GetGUIThreadInfo(0, &mut gi).is_err() {
                return None;
            }
        }
        Some(gi)
    }

    fn hwnd_screen_rect(hwnd: HWND) -> Option<RECT> {
        if hwnd.0.is_null() {
            return None;
        }
        let mut rc = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut rc).ok()?;
        }
        if rc.right - rc.left < 4 || rc.bottom - rc.top < 4 {
            return None;
        }
        Some(rc)
    }

    fn caret_screen_rect() -> Option<RECT> {
        let gi = gui_thread_info()?;
        if gi.hwndCaret.0.is_null() {
            return None;
        }
        let rc = gi.rcCaret;
        let mut tl = POINT {
            x: rc.left,
            y: rc.top,
        };
        let mut br = POINT {
            x: rc.right,
            y: rc.bottom,
        };
        unsafe {
            let _ = ClientToScreen(gi.hwndCaret, &mut tl);
            let _ = ClientToScreen(gi.hwndCaret, &mut br);
        }
        if br.x <= tl.x {
            br.x = tl.x + 2;
        }
        if br.y <= tl.y {
            br.y = tl.y + 16;
        }
        Some(RECT {
            left: tl.x,
            top: tl.y,
            right: br.x,
            bottom: br.y,
        })
    }

    /// Compact Win32 edit/combo вЂ” real text field, not a whole browser surface.
    fn compact_edit_focus() -> Option<RECT> {
        let gi = gui_thread_info()?;
        let focus = hwnd_screen_rect(gi.hwndFocus)?;
        let h = focus.bottom - focus.top;
        let w = focus.right - focus.left;
        if h <= 96 && w >= 40 && w <= 1200 {
            Some(focus)
        } else {
            None
        }
    }

    fn uia_cache_slot() -> &'static std::sync::Mutex<Option<(Instant, Option<RECT>)>> {
        use std::sync::{Mutex, OnceLock};
        static CACHE: OnceLock<Mutex<Option<(Instant, Option<RECT>)>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(None))
    }

    fn invalidate_uia_input_cache() {
        let mut guard = uia_cache_slot().lock().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    fn uia_input_anchor_cached(max_age_ms: u64) -> Option<RECT> {
        let mut guard = uia_cache_slot().lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, rect)) = guard.as_ref() {
            if at.elapsed() < Duration::from_millis(max_age_ms) {
                return *rect;
            }
        }
        let rect = crate::field_text::focused_input_rect().map(|r| RECT {
            left: r.left,
            top: r.top,
            right: r.right.max(r.left + 2),
            bottom: r.bottom.max(r.top + 12),
        });
        *guard = Some((Instant::now(), rect));
        rect
    }

    /// True while a text caret or edit field has focus (mouse may be elsewhere).
    fn text_input_engaged() -> bool {
        caret_screen_rect().is_some()
            || compact_edit_focus().is_some()
            || uia_input_anchor_cached(200).is_some()
    }

    /// Fresh anchor from OS. Win32 caret first (real line/paragraph), then UIA.
    /// Never the free mouse cursor.
    fn live_input_anchor() -> Option<RECT> {
        if let Some(caret) = caret_screen_rect() {
            return Some(caret);
        }
        let age = if typing_keys_active() { 35 } else { 70 };
        if let Some(uia) = uia_input_anchor_cached(age) {
            return Some(uia);
        }
        compact_edit_focus()
    }

    /// Point near cursor only used to seed a lock on click (not for continuous chase).
    fn cursor_seed_anchor() -> Option<RECT> {
        let (mx, my) = cursor_pos()?;
        Some(RECT {
            left: mx - 1,
            top: my - 10,
            right: mx + 1,
            bottom: my + 10,
        })
    }

    fn rect_area(r: RECT) -> i64 {
        ((r.right - r.left).max(0) as i64) * ((r.bottom - r.top).max(0) as i64)
    }

    fn is_window_maximized(hwnd: HWND) -> bool {
        unsafe { IsZoomed(hwnd).as_bool() }
    }

    /// Hop near a click without sitting on the control.
    fn dock_near_point(
        px: i32,
        py: i32,
        ww: i32,
        wh: i32,
        work: RECT,
        seed: u64,
    ) -> (i32, i32, &'static str) {
        let margin = 20;
        let order = match seed % 4 {
            0 => [0usize, 1, 2, 3],
            1 => [1, 0, 3, 2],
            2 => [2, 3, 0, 1],
            _ => [3, 2, 1, 0],
        };
        let candidates = [
            (px + margin, py - wh / 2, "left"),
            (px - ww - margin, py - wh / 2, "right"),
            (px - ww / 2, py - wh - margin, "down"),
            (px - ww / 2, py + margin, "up"),
        ];
        for idx in order {
            let (x, y, dir) = candidates[idx];
            let (x, y) = clamp_pos(x, y, ww, wh, work);
            if fits_in_work(x, y, ww, wh, work) {
                return (x, y, dir);
            }
        }
        let (x, y) = clamp_pos(px + margin, py - wh / 2, ww, wh, work);
        (x, y, "left")
    }

    fn work_area_for(x: i32, y: i32) -> RECT {
        let pt = POINT { x, y };
        unsafe {
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(mon, &mut mi).as_bool() {
                return mi.rcWork;
            }
        }
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        }
    }

    fn clamp_pos(x: i32, y: i32, w: i32, h: i32, work: RECT) -> (i32, i32) {
        let max_x = work.right - w;
        let max_y = work.bottom - h;
        (
            x.clamp(work.left, max_x.max(work.left)),
            y.clamp(work.top, max_y.max(work.top)),
        )
    }

    fn overlaps(x: i32, y: i32, w: i32, h: i32, r: RECT) -> bool {
        x < r.right && x + w > r.left && y < r.bottom && y + h > r.top
    }

    /// Typical player + controls zone (left/center of a browser video page).
    fn video_danger_zone(fg: RECT) -> RECT {
        let w = (fg.right - fg.left).max(1);
        let h = (fg.bottom - fg.top).max(1);
        RECT {
            left: fg.left,
            // Skip thin top chrome; player starts below tabs.
            top: fg.top + (h as f32 * 0.06) as i32,
            // YouTube/desktop players usually occupy ~left 55вЂ“65%.
            right: fg.left + (w as f32 * 0.64) as i32,
            // Keep clear of bottom control bar.
            bottom: fg.bottom,
        }
    }

    fn fits_in_work(x: i32, y: i32, ww: i32, wh: i32, work: RECT) -> bool {
        x >= work.left && y >= work.top && x + ww <= work.right && y + wh <= work.bottom
    }

    /// True outside the video window (preferred).
    fn try_dock_outside(
        fg: RECT,
        ww: i32,
        wh: i32,
        work: RECT,
        seed: u64,
    ) -> Option<(i32, i32, &'static str)> {
        try_dock_outside_margin(fg, ww, wh, work, seed, 12)
    }

    fn try_dock_outside_margin(
        fg: RECT,
        ww: i32,
        wh: i32,
        work: RECT,
        seed: u64,
        margin: i32,
    ) -> Option<(i32, i32, &'static str)> {
        let mid_y = (fg.top + fg.bottom - wh) / 2;
        let mid_x = (fg.left + fg.right - ww) / 2;
        let order = match seed % 4 {
            0 => [0usize, 1, 2, 3],
            1 => [1, 0, 3, 2],
            2 => [2, 3, 0, 1],
            _ => [3, 2, 1, 0],
        };
        let candidates = [
            (fg.right + margin, mid_y, "left"),
            (fg.left - ww - margin, mid_y, "right"),
            (mid_x, fg.bottom + margin, "up"),
            (mid_x, fg.top - wh - margin, "down"),
        ];
        for idx in order {
            let (x, y, dir) = candidates[idx];
            let (x, y) = clamp_pos(x, y, ww, wh, work);
            if fits_in_work(x, y, ww, wh, work) && !overlaps(x, y, ww, wh, fg) {
                return Some((x, y, dir));
            }
        }
        None
    }

    /// Beside the input/caret — generous gap so the pet never covers the text.
    fn dock_beside_input(
        anchor: RECT,
        ww: i32,
        wh: i32,
        work: RECT,
        seed: u64,
    ) -> (i32, i32, &'static str) {
        let pad = 96;
        let zone = RECT {
            left: anchor.left - pad,
            top: anchor.top - pad,
            right: anchor.right + pad,
            bottom: anchor.bottom + pad,
        };
        // Prefer above the field (chat composers sit at the bottom), then sides.
        // Large margins — user feedback: 56px still covered half the box.
        for margin in [140, 180, 220, 280] {
            if let Some(pos) =
                try_dock_outside_margin_order(zone, ww, wh, work, margin, &[3, 0, 1, 2])
            {
                return pos;
            }
            if let Some(pos) = try_dock_outside_margin(zone, ww, wh, work, seed, margin) {
                return pos;
            }
        }
        // Far fallback: top-right of work area, still clear of the field.
        let (x, y) = clamp_pos(work.right - ww - 24, work.top + 48, ww, wh, work);
        if !overlaps(x, y, ww, wh, zone) {
            return (x, y, "left");
        }
        let (x, y) = clamp_pos(zone.right + 160, zone.top - wh - 24, ww, wh, work);
        (x, y, "down")
    }

    /// Like `try_dock_outside_margin`, but with an explicit side order
    /// (0=right, 1=left, 2=below, 3=above of the zone).
    fn try_dock_outside_margin_order(
        fg: RECT,
        ww: i32,
        wh: i32,
        work: RECT,
        margin: i32,
        order: &[usize],
    ) -> Option<(i32, i32, &'static str)> {
        let mid_y = (fg.top + fg.bottom - wh) / 2;
        let mid_x = (fg.left + fg.right - ww) / 2;
        let candidates = [
            (fg.right + margin, mid_y, "left"),
            (fg.left - ww - margin, mid_y, "right"),
            (mid_x, fg.bottom + margin, "up"),
            (mid_x, fg.top - wh - margin, "down"),
        ];
        for &idx in order {
            let (x, y, dir) = candidates[idx];
            let (x, y) = clamp_pos(x, y, ww, wh, work);
            if fits_in_work(x, y, ww, wh, work) && !overlaps(x, y, ww, wh, fg) {
                return Some((x, y, dir));
            }
        }
        None
    }

    /// Maximized browser / no outer margin: sit in comments/chrome, never on the player.
    fn dock_video_safe(
        fg: RECT,
        ww: i32,
        wh: i32,
        work: RECT,
        seed: u64,
    ) -> (i32, i32, &'static str) {
        let margin = 14;
        let danger = video_danger_zone(fg);
        let fw = (fg.right - fg.left).max(1);
        let fh = (fg.bottom - fg.top).max(1);
        let along = ((seed / 3) % 97) as f32 / 97.0;

        // Right sidebar / comments column (look left toward video).
        let right_x = (fg.right - ww - margin)
            .max(fg.left + (fw as f32 * 0.66) as i32)
            .min(work.right - ww);
        let y_lo = fg.top + margin + 36;
        let y_hi = (fg.bottom - wh - margin - 24).max(y_lo);
        let y_span = (y_hi - y_lo).max(1) as f32;
        let right_y = y_lo + (y_span * along) as i32;

        // Top-right under tabs; top strip away from player.
        let top_x = fg.right - ww - margin;
        let top_y = fg.top + margin + 8;

        // Far right of the work area (taskbar-safe), mid height.
        let work_right_x = work.right - ww - margin;
        let work_right_y =
            work.top + ((work.bottom - work.top - wh) as f32 * (0.2 + 0.5 * along)) as i32;

        // Bottom-right above the very bottom (avoid player controls on the left).
        let br_x = fg.right - ww - margin;
        let br_y = fg.bottom - wh - margin - 56;

        let candidates = [
            (right_x, right_y, "left"),
            (top_x, top_y, "left"),
            (work_right_x, work_right_y, "left"),
            (br_x, br_y, "left"),
            // Last resort: top-center chrome only
            (fg.left + (fw - ww) / 2, fg.top + margin + 6, "down"),
        ];

        let start = (seed % candidates.len() as u64) as usize;
        for i in 0..candidates.len() {
            let (x, y, dir) = candidates[(start + i) % candidates.len()];
            let (cx, cy) = clamp_pos(x, y, ww, wh, work);
            if !overlaps(cx, cy, ww, wh, danger) {
                return (cx, cy, dir);
            }
        }

        // Absolute fallback: clamp to right-middle of work, still better than on the player.
        let (cx, cy) = clamp_pos(work.right - ww - margin, work.top + fh / 3, ww, wh, work);
        (cx, cy, "left")
    }

    /// Pick a video dock: outside the window if possible, else safe chrome zone.
    fn dock_for_video(
        fg: RECT,
        ww: i32,
        wh: i32,
        work: RECT,
        seed: u64,
    ) -> (i32, i32, &'static str) {
        if let Some(pos) = try_dock_outside(fg, ww, wh, work, seed) {
            return pos;
        }
        dock_video_safe(fg, ww, wh, work, seed)
    }

    /// Discrete dir + continuous aim vector for smooth eye tracking.
    /// `hard`: lock hard onto target (caret) вЂ” strong glance even when nearby.
    fn look_aim(
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        hard: bool,
    ) -> (&'static str, f32, f32) {
        let dx = (to_x - from_x) as f32;
        let dy = (to_y - from_y) as f32;
        let dist = (dx * dx + dy * dy).sqrt().max(0.001);
        let ux = dx / dist;
        let uy = dy / dist;
        let strength = if hard {
            // Always a clear directional stare toward the caret.
            0.95
        } else if dist < 14.0 {
            0.42
        } else {
            ((dist - 10.0) / 160.0).clamp(0.28, 1.0)
        };
        let nx = ux * strength;
        let ny = uy * strength;
        let dir = if !hard && dist < 6.0 {
            "center"
        } else if nx.abs() >= ny.abs() * 0.75 {
            if nx < 0.0 {
                "left"
            } else {
                "right"
            }
        } else if ny < 0.0 {
            "up"
        } else {
            "down"
        };
        (dir, nx, ny)
    }

    fn should_emit_look(
        last_emit: Instant,
        last_nx: f32,
        last_ny: f32,
        nx: f32,
        ny: f32,
        min_ms: u64,
    ) -> bool {
        let moved = (nx - last_nx).abs() > 0.025 || (ny - last_ny).abs() > 0.025;
        moved && last_emit.elapsed() >= Duration::from_millis(min_ms)
    }

    fn point_in_rect(x: i32, y: i32, r: RECT) -> bool {
        x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
    }

    fn is_our_window(app: &AppHandle, fg: HWND) -> bool {
        if let Some(win) = app.get_webview_window("main") {
            if let Ok(hwnd) = win.hwnd() {
                return hwnd.0 == fg.0;
            }
        }
        false
    }

    /// Smooth move only вЂ” never teleport. Far = faster glide, near = soft ease.
    fn step_toward(
        pos_x: &mut f32,
        pos_y: &mut f32,
        target_x: f32,
        target_y: f32,
        dt: f32,
    ) -> bool {
        let dx = target_x - *pos_x;
        let dy = target_y - *pos_y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.4 {
            *pos_x = target_x;
            *pos_y = target_y;
            return false;
        }

        const FAST_PX: f32 = 280.0;
        const FAR_PX: f32 = 720.0;

        let (lambda, max_speed) = if dist >= FAR_PX {
            (11.0_f32, 2200.0_f32) // long glide, still continuous
        } else if dist >= FAST_PX {
            (12.0_f32, 1600.0_f32)
        } else {
            (9.0_f32, 900.0_f32)
        };
        let alpha = 1.0 - (-lambda * dt).exp();
        let step = (dist * alpha).min(max_speed * dt).max(0.0);
        let t = (step / dist).min(1.0);
        *pos_x += dx * t;
        *pos_y += dy * t;
        true
    }

    pub fn run_loop(app: AppHandle) {
        const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 144);
        const SENSE_HZ: f32 = 48.0;

        let mut last_fg = 0isize;
        let mut last_dir = String::new();
        let mut last_nx = 0.0_f32;
        let mut last_ny = 0.0_f32;
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut compose_until = Instant::now();
        let mut locked_anchor: Option<RECT> = None;
        let mut pending_click_seed: Option<RECT> = None;
        let mut pending_click_until = Instant::now();
        // Short hop toward a click or a newly maximized / expanded window.
        let mut interest_anchor: Option<RECT> = None;
        let mut interest_until = Instant::now();
        let mut interest_look: Option<(i32, i32)> = None;
        let mut last_fg_rect: Option<RECT> = None;
        let mut last_fg_maximized = false;
        let mut lmb_was_down = false;
        let mut target: Option<(f32, f32)> = None;
        let mut dock_seed: u64 = 1;

        let mut pos_x = 0.0_f32;
        let mut pos_y = 0.0_f32;
        let mut pos_valid = false;
        let mut last_drawn = (i32::MIN, i32::MIN);
        let mut last_tick = Instant::now();
        let mut sense_accum = 0.0_f32;
        let mut ww = 120_i32;
        let mut wh = 120_i32;
        let mut digits = DigitTracker::new();
        let mut digit_offer_until = Instant::now();
        let mut last_digit_sync = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        let mut digit_empty_field_hits: u8 = 0;
        let mut spell_offer_until = Instant::now();
        let mut spell_plaque_active = false;
        let mut enter_was_down = false;
        let mut last_field_text = String::new();
        let mut last_field_type = String::new();
        let mut last_field_poll = Instant::now() - Duration::from_secs(1);
        let mut field_watch_until = Instant::now();
        let mut last_field_miss_log = Instant::now()
            .checked_sub(Duration::from_secs(10))
            .unwrap_or_else(Instant::now);
        let mut last_field_miss_reason = String::new();
        let mut last_spell_word = String::new();
        // Field text when we first attached — live-spell only checks the delta after this.
        let mut spell_baseline: Option<String> = None;
        let mut spell_baseline_hwnd: isize = 0;
        let mut last_spell_edit = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        let mut spell_check_pending = false;
        let mut last_layout_auto_word = String::new();
        let mut last_layout_auto_at = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        let mut last_spell_ignore_gen = crate::spell_ignore::generation();
        let mut last_was_video = false;
        let mut was_composing = false;
        // After Alt+Tab, don't immediately re-lock onto another field.
        let mut input_lock_blocked_until = Instant::now();
        // Glasses / "reading" only after real keypresses — not bare focus on an empty field.
        let mut last_typed_at = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);

        loop {
            let ignore_gen = crate::spell_ignore::generation();
            if ignore_gen != last_spell_ignore_gen {
                last_spell_ignore_gen = ignore_gen;
                if spell_plaque_active {
                    spell_plaque_active = false;
                    last_spell_word.clear();
                    let _ = app.emit(
                        "mote://spell-offer",
                        crate::spellcheck::SpellOfferPayload::empty(),
                    );
                    eprintln!("[mote] spell offer clear (ignore)");
                }
            }
            let frame_start = Instant::now();
            let dt = (frame_start - last_tick)
                .as_secs_f32()
                .clamp(1.0 / 1000.0, 0.05);
            last_tick = frame_start;

            let Some(win) = app.get_webview_window("main") else {
                thread::sleep(FRAME);
                continue;
            };

            let rage = super::is_rage_mode();
            let presence_paused = PRESENCE_PAUSED.load(Ordering::Relaxed) || rage;
            // Click-through empty chrome; stay interactive while dragging / raging.
            crate::hit_regions::apply(&win, presence_paused);

            // Dismiss side menu on any click outside it (including through to other apps).
            {
                let lmb_down = is_key_down(VK_LBUTTON.0 as i32);
                let lmb_click = lmb_down && !super::MENU_LMB_DOWN.load(Ordering::Relaxed);
                super::MENU_LMB_DOWN.store(lmb_down, Ordering::Relaxed);
                if lmb_click
                    && crate::hit_regions::menu_open()
                    && !crate::hit_regions::cursor_over_menu(&win)
                {
                    let _ = app.emit("mote://menu-dismiss", ());
                }
            }

            if !super::is_enabled() || presence_paused {
                target = None;
                pos_valid = false;
                locked_anchor = None;
                interest_anchor = None;
                interest_look = None;
                if !digits.buf.is_empty() {
                    digits.clear();
                    crate::digit_actions::clear_digit_target();
                    let _ = app.emit(
                        "mote://digit-offer",
                        DigitOfferPayload {
                            digits: String::new(),
                        },
                    );
                }
                thread::sleep(FRAME);
                continue;
            }

            // Compose session: stay on the input while caret/edit focus lives,
            // even if the mouse left the field. Never chase the free cursor.
            let lmb_down = is_key_down(VK_LBUTTON.0 as i32);
            let lmb_click = lmb_down && !lmb_was_down;
            lmb_was_down = lmb_down;

            if lmb_click {
                if text_input_engaged() {
                    locked_anchor = live_input_anchor().or_else(cursor_seed_anchor);
                    compose_until = Instant::now() + Duration::from_millis(4500);
                    pending_click_until = Instant::now();
                    interest_anchor = None;
                    interest_look = None;
                } else if !is_our_window(&app, foreground_hwnd()) {
                    // Wait briefly for caret after click (Win32 edit) or typing (Electron).
                    pending_click_seed = cursor_seed_anchor();
                    pending_click_until = Instant::now() + Duration::from_millis(450);
                }
            }

            // After a click, only lock if a real text caret/field appears — never hop to the click point.
            if Instant::now() < pending_click_until {
                if let Some(live) = live_input_anchor() {
                    locked_anchor = Some(live);
                    compose_until = Instant::now() + Duration::from_millis(4500);
                    pending_click_until = Instant::now();
                    pending_click_seed = None;
                    interest_anchor = None;
                    interest_look = None;
                }
            } else {
                pending_click_seed = None;
            }

            // Shoo / user dismiss while reading: leave the caret, drop glasses lock.
            if CLEAR_INPUT_LOCK.swap(false, Ordering::Relaxed) {
                locked_anchor = None;
                pending_click_seed = None;
                compose_until = Instant::now();
                input_lock_blocked_until = Instant::now() + Duration::from_millis(4_500);
                invalidate_uia_input_cache();
                was_composing = false;
                let _ = app.emit(
                    "mote://look",
                    LookPayload {
                        dir: "center".into(),
                        mode: "untarget".into(),
                        nx: 0.0,
                        ny: 0.0,
                    },
                );
            }

            if typing_keys_active() {
                last_typed_at = Instant::now();
            }

            if Instant::now() >= input_lock_blocked_until {
                // Reading lock only after real keypresses — never from bare focus
                // on a search box in Music / browser chrome.
                let keys_recent =
                    typing_keys_active() || last_typed_at.elapsed() < Duration::from_millis(900);
                let canvas = crate::canvas_typing::is_active();
                let canvas_typing = crate::canvas_typing::is_composing();
                // Digit plaque without UIA (non-canvas apps).
                let digit_canvas = !digits.buf.is_empty() && !canvas;
                if canvas {
                    // Paint.NET: glasses only after canvas click + real word/digits buffer.
                    if canvas_typing {
                        if let Some(seed) = cursor_seed_anchor() {
                            locked_anchor = Some(seed);
                            compose_until = Instant::now() + Duration::from_millis(2000);
                        }
                        interest_anchor = None;
                        interest_look = None;
                    } else {
                        locked_anchor = None;
                        compose_until = Instant::now();
                    }
                } else if keys_recent || digit_canvas {
                    if let Some(live) = live_input_anchor() {
                        locked_anchor = Some(live);
                        compose_until = Instant::now() + Duration::from_millis(4500);
                        interest_anchor = None;
                        interest_look = None;
                    } else if digit_canvas {
                        if let Some(seed) = cursor_seed_anchor() {
                            locked_anchor = Some(seed);
                            compose_until = if Instant::now() < digit_offer_until {
                                digit_offer_until
                            } else {
                                Instant::now() + Duration::from_millis(1500)
                            };
                        }
                        interest_anchor = None;
                        interest_look = None;
                    } else {
                        compose_until = Instant::now() + Duration::from_millis(4000);
                        if locked_anchor.is_none() {
                            if let Some(seed) = pending_click_seed.take() {
                                locked_anchor = Some(seed);
                            }
                        }
                        interest_anchor = None;
                        interest_look = None;
                    }
                } else if locked_anchor.is_some() {
                    locked_anchor = None;
                    compose_until = Instant::now();
                }
            }

            if Instant::now() >= compose_until && !text_input_engaged() {
                locked_anchor = None;
            }
            if Instant::now() >= interest_until {
                interest_anchor = None;
                interest_look = None;
            }

            let fg_now = foreground_hwnd();
            // Drop compose when user Alt+Tabs away (before we decide this frame's mode).
            let fg_id_now = fg_now.0 as isize;
            if fg_id_now != 0 && last_fg != 0 && fg_id_now != last_fg {
                locked_anchor = None;
                pending_click_seed = None;
                compose_until = Instant::now();
                last_typed_at = Instant::now()
                    .checked_sub(Duration::from_secs(60))
                    .unwrap_or_else(Instant::now);
                last_field_text.clear();
                last_field_type.clear();
                last_field_miss_reason.clear();
                last_spell_word.clear();
                spell_baseline = None;
                spell_baseline_hwnd = 0;
                spell_check_pending = false;
                spell_plaque_active = false;
                was_composing = false;
                input_lock_blocked_until = Instant::now() + Duration::from_millis(800);
                invalidate_uia_input_cache();
                let _ = app.emit(
                    "mote://spell-offer",
                    crate::spellcheck::SpellOfferPayload::empty(),
                );
                let _ = app.emit(
                    "mote://look",
                    LookPayload {
                        dir: "center".into(),
                        mode: "untarget".into(),
                        nx: 0.0,
                        ny: 0.0,
                    },
                );
            }

            // Still in-field only while keys were recent; don't stick to Music search boxes.
            let composing = if crate::canvas_typing::is_active() {
                locked_anchor.is_some() && crate::canvas_typing::is_composing()
            } else {
                locked_anchor.is_some()
                    && Instant::now() < compose_until
                    && (typing_keys_active()
                        || last_typed_at.elapsed() < Duration::from_millis(900)
                        || !digits.buf.is_empty())
            };
            // Left the text field / switched apps — tell the UI to drop glasses.
            if was_composing && !composing {
                last_dir = "center".into();
                last_nx = 0.0;
                last_ny = 0.0;
                last_emit = Instant::now();
                let _ = app.emit(
                    "mote://look",
                    LookPayload {
                        dir: "center".into(),
                        mode: "untarget".into(),
                        nx: 0.0,
                        ny: 0.0,
                    },
                );
            }
            was_composing = composing;

            // Chat send / newline: drop the spell plaque (field often goes empty or unread).
            let enter_down = is_key_down(VK_RETURN.0 as i32);
            if enter_down && !enter_was_down && spell_plaque_active {
                eprintln!("[mote] spell offer clear (enter)");
                spell_plaque_active = false;
                last_spell_word.clear();
                let _ = app.emit(
                    "mote://spell-offer",
                    crate::spellcheck::SpellOfferPayload::empty(),
                );
            }
            enter_was_down = enter_down;

            if !is_our_window(&app, fg_now) {
                // Paint.NET owns digits via canvas_typing (after click). Global DigitTracker
                // would flash plaques on tool shortcuts / random number keys.
                if crate::canvas_typing::is_active() {
                    if !digits.buf.is_empty() {
                        digits.clear();
                        crate::digit_actions::clear_digit_target();
                        let _ = app.emit(
                            "mote://digit-offer",
                            DigitOfferPayload {
                                digits: String::new(),
                            },
                        );
                    }
                } else {
                    digits.poll();
                }
                // Reconcile keyboard run with field — but never wipe digits on a laggy empty read
                // (that made Mote ignore new keystrokes entirely).
                if !crate::canvas_typing::is_active()
                    && !digits.buf.is_empty()
                    && last_digit_sync.elapsed() >= Duration::from_millis(350)
                {
                    last_digit_sync = Instant::now();
                    match crate::field_text::read_focused_field_once() {
                        Ok(snap) => match trailing_digit_run(&snap.text) {
                            Some(field_digits) => {
                                digit_empty_field_hits = 0;
                                digits.sync_from_field(&field_digits);
                            }
                            None => {
                                let has_any_digit = snap.text.chars().any(|c| c.is_ascii_digit());
                                if !snap.text.trim().is_empty() && !has_any_digit {
                                    // Typed letters/words — drop digit offer.
                                    digit_empty_field_hits = 0;
                                    digits.clear();
                                } else if snap.text.trim().is_empty() {
                                    // After Backspace/Ctrl+Backspace trust an empty field quickly.
                                    if digits.erased_recently() {
                                        digit_empty_field_hits = 0;
                                        digits.clear();
                                    } else {
                                        digit_empty_field_hits =
                                            digit_empty_field_hits.saturating_add(1);
                                        if digit_empty_field_hits >= 2 {
                                            digit_empty_field_hits = 0;
                                            digits.clear();
                                        }
                                    }
                                } else {
                                    digit_empty_field_hits = 0;
                                }
                            }
                        },
                        Err(_) => {
                            // Miss: keep keyboard buffer (Telegram/Chrome lag).
                            digit_empty_field_hits = 0;
                        }
                    }
                }
                if let Some(buf) = digits.take_dirty() {
                    if !buf.is_empty() {
                        digit_offer_until = Instant::now() + Duration::from_millis(5000);
                        spell_plaque_active = false;
                        // Prefer the focused edit control so replace lands in the field.
                        let target = gui_thread_info()
                            .map(|gi| gi.hwndFocus)
                            .filter(|h| !h.0.is_null())
                            .unwrap_or(fg_now);
                        crate::digit_actions::set_digit_target(target.0 as isize, &buf);
                    } else {
                        crate::digit_actions::clear_digit_target();
                    }
                    let _ = app.emit("mote://digit-offer", DigitOfferPayload { digits: buf });
                    // Digits win over spell plaque.
                    let _ = app.emit(
                        "mote://spell-offer",
                        crate::spellcheck::SpellOfferPayload::empty(),
                    );
                }

                // Spellcheck: read the real focused field (UIA).
                if typing_keys_active() || composing {
                    field_watch_until = Instant::now() + Duration::from_millis(2500);
                }
                // Paint / Paint.NET: canvas_typing owns spell offers there (no UIA field).
                if digits.buf.is_empty()
                    && !crate::canvas_typing::is_active()
                    && Instant::now() < field_watch_until
                    && last_field_poll.elapsed() >= Duration::from_millis(320)
                {
                    last_field_poll = Instant::now();
                    match crate::field_text::read_focused_field_detailed() {
                        Ok(snap) => {
                            let type_changed = snap.control_type != last_field_type;
                            let text_changed = snap.text != last_field_text;
                            let new_len = snap.text.chars().count();
                            let prev_len = last_field_text.chars().count();
                            // Chromium Edit glitch: Value/Text briefly jumps to a huge blob.
                            let spike = prev_len > 0
                                && prev_len < 80
                                && new_len > prev_len.saturating_mul(4)
                                && new_len.saturating_sub(prev_len) > 100;
                            if spike {
                                if last_field_miss_reason != "spike"
                                    || last_field_miss_log.elapsed() >= Duration::from_millis(2500)
                                {
                                    eprintln!(
                                        "[mote] field read miss: spike {}→{} ({})",
                                        prev_len, new_len, snap.control_type
                                    );
                                    last_field_miss_log = Instant::now();
                                    last_field_miss_reason = "spike".into();
                                }
                            } else {
                                if type_changed || text_changed {
                                    eprintln!(
                                        "[mote] field read type={} len={} hwnd={:#x}",
                                        snap.control_type, new_len, snap.hwnd
                                    );
                                    last_field_type = snap.control_type.clone();
                                }
                                if text_changed {
                                    let prev_field = last_field_text.clone();
                                    last_field_text = snap.text.clone();
                                    last_field_miss_reason.clear();
                                    last_spell_edit = Instant::now();
                                    // Sent / cleared draft — don't keep a stale plaque over the pet.
                                    if snap.text.trim().is_empty() {
                                        spell_baseline = Some(String::new());
                                        spell_baseline_hwnd = snap.hwnd;
                                        spell_check_pending = false;
                                        last_layout_auto_word.clear();
                                        if spell_plaque_active {
                                            eprintln!("[mote] spell offer clear (field empty)");
                                            spell_plaque_active = false;
                                            last_spell_word.clear();
                                            let _ = app.emit(
                                                "mote://spell-offer",
                                                crate::spellcheck::SpellOfferPayload::empty(),
                                            );
                                        }
                                    } else {
                                        if spell_baseline.is_none()
                                            || spell_baseline_hwnd != snap.hwnd
                                        {
                                            spell_baseline_hwnd = snap.hwnd;
                                            // Big first snapshot vs empty prev = opened existing file.
                                            if prev_field.is_empty()
                                                && snap.text.chars().count() > 80
                                            {
                                                spell_baseline = Some(snap.text.clone());
                                                spell_check_pending = false;
                                                eprintln!(
                                                    "[mote] spell baseline set len={} (file)",
                                                    snap.text.chars().count()
                                                );
                                            } else {
                                                // Baseline = text before this edit so the keystroke is in delta.
                                                spell_baseline = Some(prev_field);
                                                spell_check_pending = true;
                                                eprintln!(
                                                    "[mote] spell baseline set len={}",
                                                    spell_baseline
                                                        .as_ref()
                                                        .map(|s| s.chars().count())
                                                        .unwrap_or(0)
                                                );
                                            }
                                        } else if let Some(base) = spell_baseline.as_ref() {
                                            // Large delete / rewrite — re-baseline.
                                            if snap.text.chars().count() + 24 < base.chars().count()
                                            {
                                                spell_baseline = Some(snap.text.clone());
                                                spell_check_pending = false;
                                                last_layout_auto_word.clear();
                                                eprintln!("[mote] spell baseline reset (shrink)");
                                            } else {
                                                let scope = crate::spellcheck::live_spell_scope(
                                                    base, &snap.text,
                                                );
                                                // Paste / layout_auto / Cursor chat: field no longer
                                                // starts with baseline → empty delta forever.
                                                if scope.delta.is_empty() && snap.text != *base {
                                                    spell_baseline = Some(String::new());
                                                    spell_check_pending =
                                                        !snap.text.trim().is_empty();
                                                    last_layout_auto_word.clear();
                                                    eprintln!(
                                                        "[mote] spell baseline reset (diverge)"
                                                    );
                                                } else {
                                                    if crate::spellcheck::delta_mid_word(
                                                        &scope.delta,
                                                    ) && spell_plaque_active
                                                        && !crate::spellcheck::offered_word_intact(
                                                            &snap.text,
                                                            &last_spell_word,
                                                        )
                                                    {
                                                        eprintln!(
                                                            "[mote] spell offer clear (typing)"
                                                        );
                                                        spell_plaque_active = false;
                                                        last_spell_word.clear();
                                                        let _ = app.emit(
                                                            "mote://spell-offer",
                                                            crate::spellcheck::SpellOfferPayload::empty(
                                                            ),
                                                        );
                                                    }
                                                    spell_check_pending =
                                                        !scope.delta.trim().is_empty();
                                                }
                                            }
                                        }
                                    }
                                }
                                // Idle / word-boundary: spell only the typed delta.
                                // Layout (boundary already closed): ~220ms; spell: 520ms / bare 900ms.
                                let spell_idle = last_spell_edit.elapsed();
                                if spell_check_pending && !snap.text.trim().is_empty() {
                                    if let Some(base) = spell_baseline.clone() {
                                        let scope =
                                            crate::spellcheck::live_spell_scope(&base, &snap.text);
                                        let layout_ready =
                                            crate::layout_fix::delta_ready_for_layout_auto(
                                                &scope.delta,
                                            );
                                        let layout_boundary =
                                            crate::layout_fix::delta_has_layout_boundary(
                                                &scope.delta,
                                            );
                                        // Boundary → snappy; no space yet → wait a bit (mid-word).
                                        let idle_gate = if layout_ready && layout_boundary {
                                            spell_idle >= Duration::from_millis(220)
                                        } else if layout_ready {
                                            spell_idle >= Duration::from_millis(650)
                                        } else {
                                            spell_idle >= Duration::from_millis(520)
                                        };
                                        if !idle_gate {
                                            // Keep waiting.
                                        } else {
                                            let ready =
                                                crate::spellcheck::delta_ready_for_live_spell(
                                                    &scope.delta,
                                                ) || (spell_idle >= Duration::from_millis(900)
                                                    && crate::spellcheck::delta_ready_after_idle(
                                                        &scope.delta,
                                                    ))
                                                    || layout_ready;
                                            if !ready {
                                                // Keep waiting for a longer idle token / boundary.
                                            } else {
                                                spell_check_pending = false;
                                                eprintln!(
                                                    "[mote] spell delta len={} (full={})",
                                                    scope.delta.chars().count(),
                                                    snap.text.chars().count()
                                                );
                                                // Layout before spell. Do not wait for plaque: «хмм» offer
                                                // used to block EN→RU for the rest of the line forever.
                                                let mut layout_wrote = false;
                                                if layout_ready
                                                    && last_layout_auto_at.elapsed()
                                                        >= Duration::from_millis(280)
                                                {
                                                    if let Some((fixed, from, to)) =
                                                        crate::layout_fix::try_auto_replace(
                                                            &app, &snap.text,
                                                        )
                                                    {
                                                        let same_recent = from
                                                            == last_layout_auto_word
                                                            && last_layout_auto_at.elapsed()
                                                                < Duration::from_secs(3);
                                                        if !same_recent {
                                                            let hwnd = if snap.hwnd != 0 {
                                                                snap.hwnd
                                                            } else {
                                                                gui_thread_info()
                                                                    .map(|gi| gi.hwndFocus)
                                                                    .filter(|h| !h.0.is_null())
                                                                    .unwrap_or(fg_now)
                                                                    .0
                                                                    as isize
                                                            };
                                                            let top = fg_now.0 as isize;
                                                            crate::digit_actions::set_selection_replace_mode(
                                                            false,
                                                        );
                                                            crate::digit_actions::set_digit_target_ex(
                                                            hwnd, top, &snap.text,
                                                        );
                                                            match crate::digit_actions::replace_spell(
                                                            &fixed, &to,
                                                        ) {
                                                            Ok(()) => {
                                                                last_layout_auto_word = from;
                                                                last_layout_auto_at =
                                                                    Instant::now();
                                                                last_field_text = fixed.clone();
                                                                spell_baseline =
                                                                    Some(String::new());
                                                                spell_check_pending = false;
                                                                layout_wrote = true;
                                                                if spell_plaque_active {
                                                                    spell_plaque_active = false;
                                                                    last_spell_word.clear();
                                                                    let _ = app.emit(
                                                                        "mote://spell-offer",
                                                                        crate::spellcheck::SpellOfferPayload::empty(),
                                                                    );
                                                                }
                                                                eprintln!(
                                                                    "[mote] layout_auto write ok"
                                                                );
                                                            }
                                                            Err(e) => {
                                                                eprintln!(
                                                                    "[mote] layout_auto write err: {e}"
                                                                );
                                                            }
                                                        }
                                                        }
                                                    }
                                                }
                                                if !layout_wrote {
                                                    match crate::spellcheck::spell_offer(
                                                        app.clone(),
                                                        scope.delta.clone(),
                                                    ) {
                                                        Ok(Some(raw)) => {
                                                            let offer =
                                                        crate::spellcheck::apply_scope_to_offer(
                                                            raw, &scope,
                                                        );
                                                            let fix_words: Vec<String> = offer
                                                                .fixes
                                                                .iter()
                                                                .map(|f| f.word.clone())
                                                                .collect();
                                                            if crate::spell_ignore::offer_fully_ignored(
                                                        &offer.word,
                                                        &fix_words,
                                                    ) {
                                                        eprintln!(
                                                            "[mote] spell offer skip ignore «{}»",
                                                            offer.word
                                                        );
                                                    } else {
                                                    eprintln!(
                                                        "[mote] spell offer: «{}» → «{}»",
                                                        offer.word,
                                                        offer
                                                            .suggestions
                                                            .first()
                                                            .map(|s| s.as_str())
                                                            .unwrap_or("")
                                                    );
                                                    last_spell_word = offer.word.clone();
                                                    spell_offer_until = Instant::now()
                                                        + Duration::from_millis(14_000);
                                                    spell_plaque_active = true;
                                                    let hwnd = if snap.hwnd != 0 {
                                                        snap.hwnd
                                                    } else {
                                                        gui_thread_info()
                                                            .map(|gi| gi.hwndFocus)
                                                            .filter(|h| !h.0.is_null())
                                                            .unwrap_or(fg_now)
                                                            .0
                                                            as isize
                                                    };
                                                    let top = fg_now.0 as isize;
                                                    crate::digit_actions::set_selection_replace_mode(
                                                        false,
                                                    );
                                                    crate::digit_actions::set_digit_target_ex(
                                                        hwnd,
                                                        top,
                                                        &offer.replaced,
                                                    );
                                                    let _ = app.emit("mote://spell-offer", offer);
                                                    }
                                                        }
                                                        Ok(None) => {
                                                            if spell_plaque_active
                                                                && !last_spell_word.is_empty()
                                                                && !snap.text.contains(
                                                                    last_spell_word
                                                                        .split(',')
                                                                        .next()
                                                                        .unwrap_or("")
                                                                        .trim(),
                                                                )
                                                            {
                                                                eprintln!(
                                                            "[mote] spell offer clear (typo gone from field)"
                                                        );
                                                                spell_plaque_active = false;
                                                                last_spell_word.clear();
                                                                let _ = app.emit(
                                                            "mote://spell-offer",
                                                            crate::spellcheck::SpellOfferPayload::empty(),
                                                        );
                                                            }
                                                        }
                                                        Err(e) => {
                                                            eprintln!(
                                                                "[mote] spell_offer failed: {e}"
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(reason) => {
                            // Throttle: same reason ≤1 / 2.5s; don't spam every 320ms poll.
                            if reason != last_field_miss_reason.as_str()
                                || last_field_miss_log.elapsed() >= Duration::from_millis(2500)
                            {
                                let diag = crate::field_text::focus_diag_line();
                                eprintln!("[mote] field read miss: {reason}");
                                if !diag.is_empty() {
                                    eprintln!("[mote] field read diag: {diag}");
                                }
                                last_field_miss_log = Instant::now();
                                last_field_miss_reason = reason.to_string();
                            }
                        }
                    }
                }
            }
            // Auto-dismiss offer a few seconds after the last digit.
            if !digits.buf.is_empty() && Instant::now() >= digit_offer_until {
                digits.clear();
                crate::digit_actions::clear_digit_target();
                let _ = app.emit(
                    "mote://digit-offer",
                    DigitOfferPayload {
                        digits: String::new(),
                    },
                );
            }
            if spell_plaque_active && Instant::now() >= spell_offer_until {
                spell_plaque_active = false;
                last_spell_word.clear();
                let _ = app.emit(
                    "mote://spell-offer",
                    crate::spellcheck::SpellOfferPayload::empty(),
                );
            }

            sense_accum += dt;
            let do_sense = sense_accum >= 1.0 / SENSE_HZ;
            if do_sense {
                sense_accum = 0.0;
            }

            if do_sense {
                let fg_hwnd = foreground_hwnd();
                if is_our_window(&app, fg_hwnd) {
                    target = None;
                    let elapsed = frame_start.elapsed();
                    if elapsed < FRAME {
                        thread::sleep(FRAME - elapsed);
                    }
                    continue;
                }

                let fg_id = fg_hwnd.0 as isize;
                let fg_changed = fg_id != 0 && fg_id != last_fg;
                if fg_changed {
                    last_fg = fg_id;
                    dock_seed = dock_seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(fg_id as u64)
                        .wrapping_add(unsafe { GetTickCount() } as u64);
                    last_fg_rect = None;
                    last_fg_maximized = false;
                }

                if composing && !win.is_visible().unwrap_or(false) {
                    let _ = win.show();
                    let _ = win.set_always_on_top(true);
                }

                if !win.is_visible().unwrap_or(false) {
                    target = None;
                    pos_valid = false;
                    let elapsed = frame_start.elapsed();
                    if elapsed < FRAME {
                        thread::sleep(FRAME - elapsed);
                    }
                    continue;
                }

                if let Ok(size) = win.outer_size() {
                    ww = size.width as i32;
                    wh = size.height as i32;
                }

                if !pos_valid {
                    if let Ok(outer) = win.outer_position() {
                        pos_x = outer.x as f32;
                        pos_y = outer.y as f32;
                        last_drawn = (outer.x, outer.y);
                        pos_valid = true;
                    }
                }

                let cx = pos_x as i32 + ww / 2;
                let cy = pos_y as i32 + wh / 2;
                let info = foreground_info();

                // Maximize / big expand в†’ hop beside that window.
                if !composing {
                    if let Some((fg_hwnd, fg_rc, _, _)) = &info {
                        let maximized = is_window_maximized(*fg_hwnd);
                        let grew = last_fg_rect
                            .map(|prev| {
                                let da = rect_area(*fg_rc) - rect_area(prev);
                                let dw = (fg_rc.right - fg_rc.left) - (prev.right - prev.left);
                                let dh = (fg_rc.bottom - fg_rc.top) - (prev.bottom - prev.top);
                                da > 400_000 || dw > 280 || dh > 220
                            })
                            .unwrap_or(false);
                        let became_max = maximized && !last_fg_maximized;
                        if became_max || (grew && !fg_changed) {
                            interest_anchor = Some(*fg_rc);
                            interest_look = Some((
                                (fg_rc.left + fg_rc.right) / 2,
                                (fg_rc.top + fg_rc.bottom) / 2,
                            ));
                            interest_until = Instant::now() + Duration::from_millis(4200);
                            super::mark_user_placed(0);
                        }
                        last_fg_rect = Some(*fg_rc);
                        last_fg_maximized = maximized;
                    }
                }

                let interested_now =
                    !composing && interest_anchor.is_some() && Instant::now() < interest_until;

                if let (true, Some(anchor)) = (composing, locked_anchor) {
                    // Prefer the caret bar itself (left edge), not the box center.
                    let ax = anchor.left + ((anchor.right - anchor.left) / 2).min(2);
                    let ay = (anchor.top + anchor.bottom) / 2;

                    // Beside the field — never sit on the text (Cursor chat, etc.).
                    let work = work_area_for(ax, ay);
                    let (nx, ny, _) = dock_beside_input(anchor, ww, wh, work, dock_seed);
                    target = Some((nx as f32, ny as f32));

                    let eye_x = pos_x as i32 + ww / 2;
                    let eye_y = pos_y as i32 + (wh as f32 * 0.78) as i32;
                    let (dir, lnx, lny) = look_aim(eye_x, eye_y, ax, ay, true);

                    let caret_moved = should_emit_look(last_emit, last_nx, last_ny, lnx, lny, 8);
                    let heartbeat = last_emit.elapsed() >= Duration::from_millis(140);
                    if caret_moved || heartbeat || dir != last_dir {
                        last_dir = dir.to_string();
                        last_nx = lnx;
                        last_ny = lny;
                        last_emit = Instant::now();
                        let _ = app.emit(
                            "mote://look",
                            LookPayload {
                                dir: dir.into(),
                                mode: "type".into(),
                                nx: lnx,
                                ny: lny,
                            },
                        );
                    }
                } else if let (true, Some(anchor)) = (interested_now, interest_anchor) {
                    let ax = (anchor.left + anchor.right) / 2;
                    let ay = (anchor.top + anchor.bottom) / 2;
                    let work = work_area_for(ax, ay);
                    let wide = (anchor.right - anchor.left) > 120;
                    let (nx, ny, _) = if wide {
                        if let Some(pos) = try_dock_outside(anchor, ww, wh, work, dock_seed) {
                            (pos.0, pos.1, pos.2)
                        } else {
                            dock_near_point(ax, ay, ww, wh, work, dock_seed)
                        }
                    } else {
                        dock_near_point(ax, ay, ww, wh, work, dock_seed)
                    };
                    target = Some((nx as f32, ny as f32));

                    let (lx, ly) = interest_look.unwrap_or((ax, ay));
                    let (dir, lnx, lny) = look_aim(cx, cy, lx, ly, false);
                    if should_emit_look(last_emit, last_nx, last_ny, lnx, lny, 28)
                        || dir != last_dir
                    {
                        last_dir = dir.to_string();
                        last_nx = lnx;
                        last_ny = lny;
                        last_emit = Instant::now();
                        let _ = app.emit(
                            "mote://look",
                            LookPayload {
                                dir: dir.into(),
                                mode: "interest".into(),
                                nx: lnx,
                                ny: lny,
                            },
                        );
                    }
                } else if let Some((fg_hwnd, fg_rc, title, exe)) = &info {
                    let work = work_area_for(
                        (fg_rc.left + fg_rc.right) / 2,
                        (fg_rc.top + fg_rc.bottom) / 2,
                    );
                    let video = is_video_window(title, exe, *fg_hwnd);

                    if fg_changed {
                        if video {
                            let _ = app.emit(
                                "mote://activate",
                                ActivatePayload {
                                    reason: "video".into(),
                                    video: true,
                                },
                            );
                        } else if last_was_video {
                            // Leaving a video tab/app вЂ” clear В«РЎРјРѕС‚СЂСЋ СЂСЏРґРѕРјВ».
                            let _ = app.emit(
                                "mote://activate",
                                ActivatePayload {
                                    reason: "leave-video".into(),
                                    video: false,
                                },
                            );
                        }
                        last_was_video = video;
                    } else if !title.is_empty() && video != last_was_video {
                        // Same browser window, user switched tab (title changed).
                        last_was_video = video;
                        let _ = app.emit(
                            "mote://activate",
                            ActivatePayload {
                                reason: if video {
                                    "video".into()
                                } else {
                                    "leave-video".into()
                                },
                                video,
                            },
                        );
                    }

                    if video {
                        let (nx, ny, dock_dir) = dock_for_video(*fg_rc, ww, wh, work, dock_seed);
                        target = Some((nx as f32, ny as f32));

                        // Always look toward the player (danger zone center), not away.
                        let vx = (fg_rc.left + video_danger_zone(*fg_rc).right) / 2;
                        let vy = (fg_rc.top + fg_rc.bottom) / 2;
                        let (dir, lnx, lny) = if let Some((mx, my)) = cursor_pos() {
                            if !point_in_rect(mx, my, video_danger_zone(*fg_rc))
                                && point_in_rect(mx, my, *fg_rc)
                            {
                                look_aim(cx, cy, vx, vy, false)
                            } else if !point_in_rect(mx, my, *fg_rc) {
                                look_aim(cx, cy, mx, my, false)
                            } else {
                                look_aim(cx, cy, vx, vy, false)
                            }
                        } else {
                            let (d, x, y) = look_aim(cx, cy, vx, vy, false);
                            if d == "center" {
                                (dock_dir, -0.6, 0.0)
                            } else {
                                (d, x, y)
                            }
                        };

                        if fg_changed
                            || overlaps(pos_x as i32, pos_y as i32, ww, wh, *fg_rc)
                            || should_emit_look(last_emit, last_nx, last_ny, lnx, lny, 40)
                            || dir != last_dir
                        {
                            last_dir = dir.to_string();
                            last_nx = lnx;
                            last_ny = lny;
                            last_emit = Instant::now();
                            let _ = app.emit(
                                "mote://look",
                                LookPayload {
                                    dir: dir.into(),
                                    mode: "dock".into(),
                                    nx: lnx,
                                    ny: lny,
                                },
                            );
                        }
                    } else {
                        // Default: eyes follow the cursor; body stays put.
                        target = None;
                        if let Some((mx, my)) = cursor_pos() {
                            let (dir, lnx, lny) = look_aim(cx, cy, mx, my, false);
                            let moved = (mx - cx).abs() > 18 || (my - cy).abs() > 18;
                            if moved
                                && (should_emit_look(last_emit, last_nx, last_ny, lnx, lny, 28)
                                    || dir != last_dir)
                            {
                                last_dir = dir.to_string();
                                last_nx = lnx;
                                last_ny = lny;
                                last_emit = Instant::now();
                                let _ = app.emit(
                                    "mote://look",
                                    LookPayload {
                                        dir: dir.into(),
                                        mode: "follow".into(),
                                        nx: lnx,
                                        ny: lny,
                                    },
                                );
                            }
                        }
                    }
                } else {
                    target = None;
                    if let Some((mx, my)) = cursor_pos() {
                        let (dir, lnx, lny) = look_aim(cx, cy, mx, my, false);
                        let moved = (mx - cx).abs() > 18 || (my - cy).abs() > 18;
                        if moved
                            && (should_emit_look(last_emit, last_nx, last_ny, lnx, lny, 28)
                                || dir != last_dir)
                        {
                            last_dir = dir.to_string();
                            last_nx = lnx;
                            last_ny = lny;
                            last_emit = Instant::now();
                            let _ = app.emit(
                                "mote://look",
                                LookPayload {
                                    dir: dir.into(),
                                    mode: "follow".into(),
                                    nx: lnx,
                                    ny: lny,
                                },
                            );
                        }
                    }
                }
            }

            // Manual drag / shoo: don't pull back — including while reading (glasses).
            if super::is_user_placed() {
                target = None;
            }

            // Animate every frame toward the last sensed target.
            if let (true, Some((tx, ty))) = (pos_valid, target) {
                if step_toward(&mut pos_x, &mut pos_y, tx, ty, dt) {
                    let nx = pos_x.round() as i32;
                    let ny = pos_y.round() as i32;
                    if (nx, ny) != last_drawn {
                        last_drawn = (nx, ny);
                        let _ = win.set_position(PhysicalPosition::new(nx, ny));
                    }
                }
            } else if !pos_valid {
                // keep ready for next approach
            } else if target.is_none() {
                // Idle: trust window position after drag / external moves.
                if do_sense {
                    if let Ok(outer) = win.outer_position() {
                        pos_x = outer.x as f32;
                        pos_y = outer.y as f32;
                        last_drawn = (outer.x, outer.y);
                    }
                }
            }

            let elapsed = frame_start.elapsed();
            if elapsed < FRAME {
                thread::sleep(FRAME - elapsed);
            }
        }
    }

    pub fn shoo_away(app: &AppHandle) -> Result<(), String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let win = app
            .get_webview_window("main")
            .ok_or_else(|| "РѕРєРЅРѕ РЅРµ РЅР°Р№РґРµРЅРѕ".to_string())?;
        let outer = win.outer_position().map_err(|e| e.to_string())?;
        let size = win.outer_size().map_err(|e| e.to_string())?;
        let ww = size.width as i32;
        let wh = size.height as i32;
        let cx = outer.x;
        let cy = outer.y;
        let work = work_area_for(cx + ww / 2, cy + wh / 2);
        let margin = 18_i32;

        let candidates = [
            (work.left + margin, work.top + margin),
            (work.right - ww - margin, work.top + margin),
            (work.left + margin, work.bottom - wh - margin),
            (work.right - ww - margin, work.bottom - wh - margin),
            ((work.left + work.right - ww) / 2, work.top + margin),
            ((work.left + work.right - ww) / 2, work.bottom - wh - margin),
            (work.left + margin, (work.top + work.bottom - wh) / 2),
            (work.right - ww - margin, (work.top + work.bottom - wh) / 2),
        ];

        let mut scored: Vec<(i64, i32, i32)> = candidates
            .into_iter()
            .map(|(x, y)| {
                let (x, y) = clamp_pos(x, y, ww, wh, work);
                let dx = (x - cx) as i64;
                let dy = (y - cy) as i64;
                (dx * dx + dy * dy, x, y)
            })
            .filter(|(d2, _, _)| *d2 > 180_i64 * 180)
            .collect();

        if scored.is_empty() {
            // Tiny monitor вЂ” just nudge diagonally.
            let (x, y) = clamp_pos(cx + 220, cy + 160, ww, wh, work);
            scored.push((1, x, y));
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let top_n = scored.len().min(4).max(1);
        let mut hasher = DefaultHasher::new();
        unsafe { GetTickCount() }.hash(&mut hasher);
        cx.hash(&mut hasher);
        let pick = scored[hasher.finish() as usize % top_n];
        let (nx, ny) = (pick.1, pick.2);

        mark_user_placed(4_500);
        clear_input_lock();
        win.set_position(PhysicalPosition::new(nx, ny))
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn enter_rage_mode(app: &AppHandle) -> Result<(), String> {
        let win = app
            .get_webview_window("main")
            .ok_or_else(|| "окно не найдено".to_string())?;
        let outer = win.outer_position().map_err(|e| e.to_string())?;
        let size = win.outer_size().map_err(|e| e.to_string())?;

        {
            let mut saved = super::RAGE_SAVED.lock().unwrap_or_else(|e| e.into_inner());
            if saved.is_none() {
                *saved = Some(super::SavedGeom {
                    x: outer.x,
                    y: outer.y,
                    w: size.width,
                    h: size.height,
                });
            }
        }

        let work = work_area_for(
            outer.x + size.width as i32 / 2,
            outer.y + size.height as i32 / 2,
        );
        let ww = (work.right - work.left).max(320) as u32;
        let wh = (work.bottom - work.top).max(240) as u32;

        super::RAGE_MODE.store(true, Ordering::Relaxed);
        super::PRESENCE_PAUSED.store(true, Ordering::Relaxed);
        clear_input_lock();
        // Hold forever until apology restores geometry.
        mark_user_placed(u64::MAX / 4);

        let _ = win.set_ignore_cursor_events(false);
        win.set_size(PhysicalSize::new(ww, wh))
            .map_err(|e| e.to_string())?;
        win.set_position(PhysicalPosition::new(work.left, work.top))
            .map_err(|e| e.to_string())?;
        let _ = win.set_always_on_top(true);
        let _ = win.set_focus();
        Ok(())
    }

    pub fn exit_rage_mode(app: &AppHandle) -> Result<(), String> {
        let win = app
            .get_webview_window("main")
            .ok_or_else(|| "окно не найдено".to_string())?;

        super::RAGE_MODE.store(false, Ordering::Relaxed);
        super::PRESENCE_PAUSED.store(false, Ordering::Relaxed);

        let saved = {
            let mut guard = super::RAGE_SAVED.lock().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };

        let geom = saved.unwrap_or(super::SavedGeom {
            x: 80,
            y: 80,
            w: 360,
            h: 280,
        });

        win.set_size(PhysicalSize::new(geom.w.max(200), geom.h.max(160)))
            .map_err(|e| e.to_string())?;
        win.set_position(PhysicalPosition::new(geom.x, geom.y))
            .map_err(|e| e.to_string())?;
        mark_user_placed(3_500);
        Ok(())
    }
}

/// Double-click shoo: jump to a far corner of the work area and stay put a while.
pub fn shoo_away(app: &AppHandle) -> Result<(), String> {
    if is_rage_mode() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        win::shoo_away(app)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("только Windows".into())
    }
}

/// Inflate over the monitor work area until the user apologizes.
pub fn enter_rage_mode(app: &AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::enter_rage_mode(app)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        RAGE_MODE.store(true, Ordering::Relaxed);
        PRESENCE_PAUSED.store(true, Ordering::Relaxed);
        Ok(())
    }
}

pub fn exit_rage_mode(app: &AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::exit_rage_mode(app)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        RAGE_MODE.store(false, Ordering::Relaxed);
        PRESENCE_PAUSED.store(false, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(windows)]
pub fn start(app: AppHandle) {
    thread::spawn(move || win::run_loop(app));
}

#[cfg(not(windows))]
pub fn start(_app: AppHandle) {}
