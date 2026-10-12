//! Copy / replace typed digits or spell fixes in the focused field.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Clone)]
struct DigitTarget {
    /// Control / focus hwnd (may be 0 for Electron).
    hwnd: isize,
    /// Top-level window to bring to foreground before SendInput.
    top_hwnd: isize,
    /// Characters to backspace for the simple word-replace fallback.
    len: usize,
}

static DIGIT_TARGET: Mutex<Option<DigitTarget>> = Mutex::new(None);
/// Replace should paste over the current selection (on-demand spell), not rewrite the field.
static SELECTION_REPLACE: AtomicBool = AtomicBool::new(false);

pub fn set_selection_replace_mode(on: bool) {
    SELECTION_REPLACE.store(on, Ordering::Relaxed);
}

pub fn set_digit_target(hwnd: isize, digits: &str) {
    set_digit_target_ex(hwnd, 0, digits);
}

pub fn set_digit_target_ex(hwnd: isize, top_hwnd: isize, digits: &str) {
    let mut guard = DIGIT_TARGET.lock().unwrap_or_else(|e| e.into_inner());
    if digits.is_empty() {
        *guard = None;
    } else {
        *guard = Some(DigitTarget {
            hwnd,
            top_hwnd,
            len: digits.chars().count().min(128),
        });
    }
}

pub fn clear_digit_target() {
    let mut guard = DIGIT_TARGET.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
    SELECTION_REPLACE.store(false, Ordering::Relaxed);
}

/// Copy words to the system clipboard.
pub fn copy_words(words: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::set_clipboard_text(words)
    }
    #[cfg(not(windows))]
    {
        let _ = words;
        Err("только Windows".into())
    }
}

/// Win32 clipboard write (used by canvas_typing).
pub fn set_clipboard(text: &str) -> Result<(), String> {
    copy_words(text)
}

/// Bring `hwnd` to the foreground via the AttachThreadInput path used by replace.
pub fn focus_window(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        win::focus_hwnd(windows::Win32::Foundation::HWND(
            hwnd as *mut core::ffi::c_void,
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

/// Focus the field where digits were typed, delete them, paste words.
pub fn replace_with_words(words: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::replace_with_words(words)
    }
    #[cfg(not(windows))]
    {
        let _ = words;
        Err("только Windows".into())
    }
}

/// Spell fix: write full corrected field text (clipboard + select-all + paste).
pub fn replace_spell(replaced: &str, suggestion: &str) -> Result<(), String> {
    // Canvas (Paint / Paint.NET) offer: retype via SendInput, not field write.
    if let Some(res) = crate::canvas_typing::try_replace() {
        return res;
    }
    #[cfg(windows)]
    {
        if SELECTION_REPLACE.load(Ordering::Relaxed) {
            let text = if !replaced.is_empty() {
                replaced
            } else {
                suggestion
            };
            return win::replace_selection(text);
        }
        win::replace_spell(replaced, suggestion)
    }
    #[cfg(not(windows))]
    {
        let _ = (replaced, suggestion);
        Err("только Windows".into())
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Controls::{EM_REPLACESEL, EM_SETSEL};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_BACK, VK_CONTROL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, BringWindowToTop, GetClassNameW, GetForegroundWindow,
        GetGUIThreadInfo, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindow, SendMessageW,
        SetForegroundWindow, ShowWindow, GUITHREADINFO, GW_OWNER, SW_RESTORE, SW_SHOW,
        WM_GETTEXTLENGTH, WM_PASTE,
    };

    fn open_clipboard_retry() -> Result<(), String> {
        unsafe {
            for _ in 0..30 {
                if OpenClipboard(None).is_ok() {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        Err("буфер обмена занят".into())
    }

    pub fn get_clipboard_text() -> Option<String> {
        unsafe {
            if IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).is_err() {
                return None;
            }
            open_clipboard_retry().ok()?;
            let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
            if handle.0.is_null() {
                let _ = CloseClipboard();
                return None;
            }
            // Same bit pattern as GlobalAlloc's HGLOBAL (HANDLE ↔ HGLOBAL).
            let hmem = std::mem::transmute_copy(&handle);
            let ptr = GlobalLock(hmem) as *const u16;
            if ptr.is_null() {
                let _ = CloseClipboard();
                return None;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
                if len > 1_000_000 {
                    break;
                }
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let text = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(hmem);
            let _ = CloseClipboard();
            Some(text)
        }
    }

    pub fn set_clipboard_text(text: &str) -> Result<(), String> {
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * std::mem::size_of::<u16>();

        unsafe {
            let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| e.to_string())?;
            let ptr = GlobalLock(hmem) as *mut u16;
            if ptr.is_null() {
                return Err("не удалось выделить буфер".into());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let _ = GlobalUnlock(hmem);

            open_clipboard_retry()?;
            let _ = EmptyClipboard();
            if SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(hmem.0))).is_err() {
                let _ = CloseClipboard();
                return Err("не удалось записать в буфер".into());
            }
            let _ = CloseClipboard();
        }
        eprintln!("[mote] clipboard set ({} chars)", text.chars().count());
        Ok(())
    }

    fn restore_clipboard(prev: Option<&str>) {
        match prev {
            Some(text) => {
                if let Err(e) = set_clipboard_text(text) {
                    eprintln!("[mote] clipboard restore err: {e}");
                } else {
                    eprintln!("[mote] clipboard restored ({} chars)", text.chars().count());
                }
            }
            None => eprintln!("[mote] clipboard restore skip (was empty)"),
        }
    }

    fn send_vk(vk: VIRTUAL_KEY, up: bool) {
        let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) as u16 };
        let flags = if up {
            KEYEVENTF_KEYUP
        } else {
            Default::default()
        };
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }

    fn tap_vk(vk: VIRTUAL_KEY) {
        send_vk(vk, false);
        thread::sleep(Duration::from_millis(12));
        send_vk(vk, true);
    }

    fn chord_ctrl(vk: VIRTUAL_KEY) {
        send_vk(VK_CONTROL, false);
        thread::sleep(Duration::from_millis(20));
        send_vk(vk, false);
        thread::sleep(Duration::from_millis(20));
        send_vk(vk, true);
        thread::sleep(Duration::from_millis(20));
        send_vk(VK_CONTROL, true);
        thread::sleep(Duration::from_millis(30));
    }

    fn paste() {
        chord_ctrl(VIRTUAL_KEY(0x56)); // V
    }

    fn select_all() {
        chord_ctrl(VIRTUAL_KEY(0x41)); // A
    }

    pub(super) fn focus_hwnd(target: HWND) -> bool {
        unsafe {
            if !IsWindow(Some(target)).as_bool() {
                return false;
            }
            if IsIconic(target).as_bool() {
                let _ = ShowWindow(target, SW_RESTORE);
            } else {
                let _ = ShowWindow(target, SW_SHOW);
            }
            let _ = BringWindowToTop(target);
            let fg = GetForegroundWindow();
            let fg_tid = GetWindowThreadProcessId(fg, None);
            let target_tid = GetWindowThreadProcessId(target, None);
            let cur = GetCurrentThreadId();
            let _ = AllowSetForegroundWindow(u32::MAX);
            if fg_tid != 0 && target_tid != 0 {
                let _ = AttachThreadInput(cur, fg_tid, true);
                let _ = AttachThreadInput(cur, target_tid, true);
                let _ = AttachThreadInput(fg_tid, target_tid, true);
            }
            let _ = SetForegroundWindow(target);
            let _ = SetFocus(Some(target));
            if fg_tid != 0 && target_tid != 0 {
                let _ = AttachThreadInput(cur, fg_tid, false);
                let _ = AttachThreadInput(cur, target_tid, false);
                let _ = AttachThreadInput(fg_tid, target_tid, false);
            }
            thread::sleep(Duration::from_millis(50));
            let now = GetForegroundWindow();
            now == target || top_level(now) == top_level(target)
        }
    }

    fn top_level(hwnd: HWND) -> HWND {
        unsafe {
            let mut cur = hwnd;
            for _ in 0..8 {
                let Ok(owner) = GetWindow(cur, GW_OWNER) else {
                    break;
                };
                if owner.0.is_null() {
                    break;
                }
                cur = owner;
            }
            cur
        }
    }

    fn resolve_focus_hwnd(preferred: isize, top: isize) -> Option<HWND> {
        if preferred != 0 {
            let hwnd = HWND(preferred as *mut core::ffi::c_void);
            unsafe {
                if IsWindow(Some(hwnd)).as_bool() {
                    return Some(hwnd);
                }
            }
        }
        unsafe {
            let mut gi = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(0, &mut gi).is_ok() && !gi.hwndFocus.0.is_null() {
                return Some(gi.hwndFocus);
            }
        }
        if top != 0 {
            let hwnd = HWND(top as *mut core::ffi::c_void);
            unsafe {
                if IsWindow(Some(hwnd)).as_bool() {
                    return Some(hwnd);
                }
            }
        }
        None
    }

    fn activate_target(stored: &DigitTarget) -> Option<HWND> {
        let top = if stored.top_hwnd != 0 {
            HWND(stored.top_hwnd as *mut core::ffi::c_void)
        } else if stored.hwnd != 0 {
            top_level(HWND(stored.hwnd as *mut core::ffi::c_void))
        } else {
            unsafe { GetForegroundWindow() }
        };
        let _ = focus_hwnd(top);
        thread::sleep(Duration::from_millis(60));
        resolve_focus_hwnd(stored.hwnd, stored.top_hwnd)
    }

    fn win32_class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        let n = unsafe { GetClassNameW(hwnd, &mut buf) };
        if n == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }

    /// Win11 Notepad RichEditD2DPT — EM_REPLACESEL / WM_GETTEXTLENGTH are unreliable.
    fn is_richedit_hwnd(hwnd: HWND) -> bool {
        let c = win32_class_name(hwnd);
        c.contains("RichEditD2DPT") || c.contains("RichEdit")
    }

    fn set_edit_text(hwnd: HWND, text: &str) -> bool {
        if is_richedit_hwnd(hwnd) {
            return false;
        }
        let before = unsafe { SendMessageW(hwnd, WM_GETTEXTLENGTH, None, None).0 };
        if before == 0 && text.is_empty() {
            return false;
        }
        // Only trust classic edit controls that already had content or accept length queries.
        if before == 0 {
            return false;
        }
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        unsafe {
            SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            SendMessageW(
                hwnd,
                EM_REPLACESEL,
                Some(WPARAM(1)),
                Some(LPARAM(wide.as_ptr() as isize)),
            );
            let after = SendMessageW(hwnd, WM_GETTEXTLENGTH, None, None).0;
            let expect = text.encode_utf16().count() as isize;
            after > 0 && (after - expect).abs() <= 2
        }
    }

    fn text_applied(hwnd: isize, text: &str, suggestion: &str) -> bool {
        // STA reads — same apartment fix as write (invoke thread is often MTA).
        let Some(got) = crate::field_text::read_field_hwnd(hwnd)
            .or_else(|| crate::field_text::read_focused_field_sta().map(|s| s.text))
        else {
            return false;
        };
        got == text
            || (!suggestion.is_empty() && got.contains(suggestion))
            || (!text.is_empty() && got.contains(text))
    }

    fn paste_via_message(hwnd: HWND) -> bool {
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return false;
            }
            SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            thread::sleep(Duration::from_millis(20));
            SendMessageW(hwnd, WM_PASTE, None, None);
            true
        }
    }

    /// Clipboard → focus → Ctrl+V over the existing selection (no Select All).
    pub fn replace_selection(text: &str) -> Result<(), String> {
        if text.is_empty() {
            return Err("нечего вставлять".into());
        }
        let stored = {
            let guard = DIGIT_TARGET.lock().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        let stored = stored.unwrap_or(DigitTarget {
            hwnd: 0,
            top_hwnd: unsafe { GetForegroundWindow().0 as isize },
            len: text.chars().count().min(128),
        });
        set_clipboard_text(text)?;
        let _ = activate_target(&stored);
        thread::sleep(Duration::from_millis(80));
        paste();
        thread::sleep(Duration::from_millis(60));
        eprintln!("[mote] spell write ok via selection Ctrl+V");
        clear_digit_target();
        Ok(())
    }

    /// Clipboard → restore target focus → Ctrl+A → Ctrl+V (and WM_PASTE fallback).
    fn paste_replaced(text: &str, stored: &DigitTarget) -> Result<(), String> {
        set_clipboard_text(text)?;

        let mut focused = false;
        for attempt in 0..3 {
            if activate_target(stored).is_some() {
                focused = true;
                break;
            }
            eprintln!("[mote] spell focus retry {attempt}");
            thread::sleep(Duration::from_millis(80));
        }
        if !focused {
            eprintln!("[mote] spell focus weak — sending chords anyway");
        }

        thread::sleep(Duration::from_millis(100));
        select_all();
        thread::sleep(Duration::from_millis(60));
        paste();
        thread::sleep(Duration::from_millis(80));

        if stored.hwnd != 0 {
            let hwnd = HWND(stored.hwnd as *mut core::ffi::c_void);
            let _ = paste_via_message(hwnd);
        }
        Ok(())
    }

    pub fn replace_with_words(words: &str) -> Result<(), String> {
        let target = {
            let guard = DIGIT_TARGET.lock().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        let Some(stored) = target else {
            return Err("нет активного числа".into());
        };
        if stored.len == 0 || words.is_empty() {
            return Err("нечего вставлять".into());
        }

        set_clipboard_text(words)?;
        let _ = activate_target(&stored);
        thread::sleep(Duration::from_millis(50));

        for _ in 0..stored.len {
            tap_vk(VK_BACK);
            thread::sleep(Duration::from_millis(6));
        }
        thread::sleep(Duration::from_millis(25));
        paste();

        clear_digit_target();
        Ok(())
    }

    pub fn replace_spell(replaced: &str, suggestion: &str) -> Result<(), String> {
        let text = if !replaced.is_empty() {
            replaced
        } else if !suggestion.is_empty() {
            suggestion
        } else {
            return Err("нечего вставлять".into());
        };

        let stored = {
            let guard = DIGIT_TARGET.lock().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        let stored = stored.unwrap_or(DigitTarget {
            hwnd: 0,
            top_hwnd: unsafe { GetForegroundWindow().0 as isize },
            len: suggestion.chars().count().min(128),
        });

        eprintln!(
            "[mote] spell_replace hwnd={:#x} top={:#x} chars={}",
            stored.hwnd as usize,
            stored.top_hwnd as usize,
            text.chars().count()
        );

        // Keep user's copy (link etc.) — layout/spell used to leave the fix on the clipboard.
        let prev_clip = get_clipboard_text();

        // Put corrected text on the clipboard for Ctrl+V / WM_PASTE fallbacks.
        set_clipboard_text(text)?;

        let focus_hwnd_opt = resolve_focus_hwnd(stored.hwnd, stored.top_hwnd);
        let richedit = focus_hwnd_opt.map(is_richedit_hwnd).unwrap_or(false)
            || (stored.hwnd != 0 && is_richedit_hwnd(HWND(stored.hwnd as *mut core::ffi::c_void)));

        let finish_ok = |via: &str| {
            eprintln!("[mote] spell write ok via {via}");
            restore_clipboard(prev_clip.as_deref());
            clear_digit_target();
            Ok(())
        };

        // Win11 Notepad: ValuePattern N/A, EM_* flaky — Ctrl+A/V + TextPattern verify.
        if richedit {
            paste_replaced(text, &stored)?;
            thread::sleep(Duration::from_millis(140));
            if text_applied(stored.hwnd, text, suggestion) {
                return finish_ok("Ctrl+V (RichEdit)");
            }
            eprintln!("[mote] RichEdit Ctrl+V not verified; trying focus UIA");
        }

        // 1) UIA by stored HWND — does not need Mote to give focus back.
        if stored.hwnd != 0 && !richedit {
            match crate::field_text::write_field_hwnd(stored.hwnd, text) {
                Ok(()) => {
                    if text_applied(stored.hwnd, text, suggestion) {
                        return finish_ok("UIA hwnd");
                    }
                    eprintln!("[mote] UIA write claimed ok but text mismatch");
                }
                Err(e) => eprintln!("[mote] UIA hwnd write: {e}"),
            }
        }

        // 2) UIA on whatever is focused after we steal FG back.
        let _ = activate_target(&stored);
        if crate::field_text::write_focused_field(text).is_ok()
            && text_applied(stored.hwnd, text, suggestion)
        {
            return finish_ok("focused UIA");
        }

        // 3) Classic Win32 Edit (skip RichEditD2DPT — handled above).
        if !richedit {
            if let Some(hwnd) = resolve_focus_hwnd(stored.hwnd, stored.top_hwnd) {
                let _ = focus_hwnd(if stored.top_hwnd != 0 {
                    HWND(stored.top_hwnd as *mut core::ffi::c_void)
                } else {
                    hwnd
                });
                thread::sleep(Duration::from_millis(40));
                let _ = focus_hwnd(hwnd);
                if set_edit_text(hwnd, text) {
                    return finish_ok("EM_REPLACESEL");
                }
                if paste_via_message(hwnd) && text_applied(hwnd.0 as isize, text, suggestion) {
                    return finish_ok("WM_PASTE");
                }
            }
        }

        // 4) Restore FG + Ctrl+A/V (Electron / Cursor / browsers / RichEdit retry).
        paste_replaced(text, &stored)?;
        thread::sleep(Duration::from_millis(120));
        if text_applied(stored.hwnd, text, suggestion) {
            return finish_ok("Ctrl+V");
        }

        // Soft success: leave fix on clipboard only if we had nothing to restore.
        eprintln!("[mote] spell_replace: field not verified; clipboard ready");
        if prev_clip.is_some() {
            restore_clipboard(prev_clip.as_deref());
        }
        clear_digit_target();
        Ok(())
    }
}
