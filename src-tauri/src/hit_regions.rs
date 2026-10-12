//! Click-through outside interactive Mote chrome (pet / bubble / menu).

use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::WebviewWindow;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

static REGIONS: Mutex<Vec<HitRect>> = Mutex::new(Vec::new());
static MENU_RECT: Mutex<Option<HitRect>> = Mutex::new(None);
static LAST_IGNORE: AtomicBool = AtomicBool::new(false);
/// Extra CSS px around each hit box so the pet is easier to grab.
const PAD: f64 = 6.0;

pub fn set_regions(regions: Vec<HitRect>, menu: Option<HitRect>) {
    let mut guard = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    *guard = regions;
    let mut m = MENU_RECT.lock().unwrap_or_else(|e| e.into_inner());
    *m = menu;
}

pub fn menu_open() -> bool {
    MENU_RECT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
}

/// True when the cursor is over the open side menu (CSS client space).
pub fn cursor_over_menu(win: &WebviewWindow) -> bool {
    let menu = {
        let guard = MENU_RECT.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    };
    let Some(r) = menu else {
        return false;
    };
    cursor_in_rect(win, &r, PAD + 2.0)
}

pub fn apply(win: &WebviewWindow, force_interactive: bool) {
    let ignore = if force_interactive {
        false
    } else {
        !cursor_over_hit(win)
    };

    if LAST_IGNORE.load(Ordering::Relaxed) == ignore {
        return;
    }
    if win.set_ignore_cursor_events(ignore).is_ok() {
        LAST_IGNORE.store(ignore, Ordering::Relaxed);
    }
}

fn cursor_over_hit(win: &WebviewWindow) -> bool {
    let regions = {
        let guard = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    };
    if regions.is_empty() {
        // Before first frontend sync — keep interactive so the pet stays usable.
        return true;
    }
    regions.iter().any(|r| cursor_in_rect(win, r, PAD))
}

fn cursor_in_rect(win: &WebviewWindow, r: &HitRect, pad: f64) -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut pt = POINT::default();
        if unsafe { GetCursorPos(&mut pt) }.is_err() {
            return true;
        }

        let Ok(inner) = win.inner_position() else {
            return true;
        };
        let scale = win.scale_factor().unwrap_or(1.0) as f64;
        if scale <= 0.0 {
            return true;
        }

        let mx = (pt.x - inner.x) as f64 / scale;
        let my = (pt.y - inner.y) as f64 / scale;

        mx >= r.x - pad && my >= r.y - pad && mx <= r.x + r.w + pad && my <= r.y + r.h + pad
    }
    #[cfg(not(windows))]
    {
        let _ = (win, r, pad);
        true
    }
}
