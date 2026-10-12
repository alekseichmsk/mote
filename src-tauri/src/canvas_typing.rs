//! Spell-check for text typed on canvases without a UIA field (MS Paint, Paint.NET).
//!
//! Low-level keyboard hook → in-memory word buffer → spellcheck on a worker thread →
//! the same `mote://spell-offer` plaque. Never auto-replaces; replace only on click.
//! Nothing typed is logged except lengths.

use tauri::AppHandle;

/// Foreground processes where canvas typing is tracked (lower-case exe names).
pub const CANVAS_ALLOWLIST: &[&str] = &["mspaint.exe", "paintdotnet.exe"];

/// True while the foreground process is in the allowlist (watcher skips UIA there).
pub fn is_active() -> bool {
    #[cfg(windows)]
    {
        win::ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// True after a canvas click + while a word/offer is in progress (glasses / digit plaque).
pub fn is_composing() -> bool {
    #[cfg(windows)]
    {
        win::COMPOSING.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[allow(dead_code)] // settings toggle later
pub fn set_enabled(enabled: bool) {
    #[cfg(windows)]
    {
        win::ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);
        if !enabled {
            win::send(win::Ev::Reset("disabled"));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
    }
}

pub fn start(app: AppHandle) {
    #[cfg(windows)]
    {
        win::start(app);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

pub fn stop() {
    #[cfg(windows)]
    {
        win::stop();
    }
}

/// `Some(result)` if a canvas offer is pending and replace was handled here.
pub fn try_replace() -> Option<Result<(), String>> {
    #[cfg(windows)]
    {
        win::try_replace()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};
    use tauri::{Emitter, Manager};
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyState, GetKeyboardLayout, SendInput, ToUnicodeEx, INPUT, INPUT_0,
        INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK,
        VK_CAPITAL, VK_CONTROL, VK_LBUTTON, VK_LWIN, VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_RWIN,
        VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetAncestor, GetCursorPos, GetForegroundWindow,
        GetMessageW, GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, WindowFromPoint, GA_ROOT, HHOOK, KBDLLHOOKSTRUCT,
        LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
    };

    pub static ENABLED: AtomicBool = AtomicBool::new(true);
    pub static ACTIVE: AtomicBool = AtomicBool::new(false);
    /// Click-to-type on canvas armed; random tool shortcuts must not put glasses on.
    static ARMED: AtomicBool = AtomicBool::new(false);
    /// ARMED and (non-empty word or pending spell offer).
    pub static COMPOSING: AtomicBool = AtomicBool::new(false);
    /// Foreground hwnd while ACTIVE (hook reads its keyboard layout).
    static ACTIVE_HWND: AtomicIsize = AtomicIsize::new(0);
    static HOOK_TID: AtomicU32 = AtomicU32::new(0);
    static TX: OnceLock<Mutex<Sender<Ev>>> = OnceLock::new();
    static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
    /// After a successful replace, worker adopts the corrected line so later typos still track.
    static AFTER_REPLACE: Mutex<Option<String>> = Mutex::new(None);

    const MAX_LINE: usize = 200;
    /// ToUnicodeEx flag: do not change keyboard state (Win10 1607+), keeps dead keys intact.
    const TU_NO_STATE_CHANGE: u32 = 0x4;

    pub enum Ev {
        Char(char),
        Back,
        Reset(&'static str),
    }

    #[derive(Clone)]
    struct Pending {
        hwnd: isize,
        /// Full keyboard line since arm/click — Backspace this many chars on Replace.
        typed_len: usize,
        correction: String,
    }

    pub fn send(ev: Ev) {
        if let Some(m) = TX.get() {
            if let Ok(tx) = m.lock() {
                let _ = tx.send(ev);
            }
        }
    }

    fn vk_down(vk: VIRTUAL_KEY) -> bool {
        unsafe { GetAsyncKeyState(i32::from(vk.0)) as u16 & 0x8000 != 0 }
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0
            && ENABLED.load(Ordering::Relaxed)
            && ACTIVE.load(Ordering::Relaxed)
            && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN)
            && lparam.0 != 0
        {
            let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if (k.flags.0 & LLKHF_INJECTED.0) == 0 {
                handle_key(k.vkCode, k.scanCode, wparam.0 as u32 == WM_SYSKEYDOWN);
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    /// Runs on the hook thread — must stay cheap (no spellcheck, no logging of text).
    fn handle_key(vk: u32, scan: u32, sys: bool) {
        match vk {
            0x08 => return send(Ev::Back),
            // Enter, Esc, Tab, PgUp/PgDn, End, Home, arrows, Delete.
            0x0D | 0x1B | 0x09 | 0x21..=0x28 | 0x2E => return send(Ev::Reset("nav key")),
            // Modifiers alone: ignore.
            0x10 | 0x11 | 0x12 | 0x14 | 0xA0..=0xA5 | 0x5B | 0x5C => return,
            _ => {}
        }
        let ctrl = vk_down(VK_CONTROL);
        let alt = vk_down(VK_MENU) || sys;
        if ctrl || alt || vk_down(VK_LWIN) || vk_down(VK_RWIN) {
            return send(Ev::Reset("shortcut"));
        }
        let mut state = [0u8; 256];
        if vk_down(VK_SHIFT) {
            state[VK_SHIFT.0 as usize] = 0x80;
        }
        if caps_on() {
            state[VK_CAPITAL.0 as usize] = 0x01;
        }
        let hwnd = HWND(ACTIVE_HWND.load(Ordering::Relaxed) as *mut core::ffi::c_void);
        let tid = unsafe { GetWindowThreadProcessId(hwnd, None) };
        let hkl = unsafe { GetKeyboardLayout(tid) };
        let mut buf = [0u16; 8];
        let n = unsafe { ToUnicodeEx(vk, scan, &state, &mut buf, TU_NO_STATE_CHANGE, Some(hkl)) };
        if n == 1 {
            if let Some(Ok(c)) = char::decode_utf16([buf[0]]).next() {
                if !c.is_control() {
                    send(Ev::Char(c));
                }
            }
        }
    }

    fn caps_on() -> bool {
        unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) & 1 != 0 }
    }

    fn proc_name(pid: u32) -> String {
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return String::new();
            };
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                h,
                Default::default(),
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .is_ok();
            let _ = CloseHandle(h);
            if !ok || len == 0 {
                return String::new();
            }
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            full.rsplit(['\\', '/'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
        }
    }

    fn start_hook_thread() {
        let _ = thread::Builder::new()
            .name("mote-canvas-hook".into())
            .spawn(|| unsafe {
                HOOK_TID.store(GetCurrentThreadId(), Ordering::SeqCst);
                let hmod = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
                let hook: HHOOK = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod, 0)
                {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("[mote] canvas: hook failed: {e}");
                        return;
                    }
                };
                eprintln!("[mote] canvas: keyboard hook installed");
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = UnhookWindowsHookEx(hook);
                eprintln!("[mote] canvas: keyboard hook removed");
            });
    }

    pub fn stop() {
        let tid = HOOK_TID.swap(0, Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn mote_hwnd(app: &AppHandle) -> isize {
        app.get_webview_window("main")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as isize)
            .unwrap_or(0)
    }

    fn clear_offer(app: &AppHandle, why: &str) {
        let had = PENDING
            .lock()
            .map(|mut g| g.take().is_some())
            .unwrap_or(false);
        if had {
            eprintln!("[mote] canvas: offer cleared ({why})");
            let _ = app.emit(
                "mote://spell-offer",
                crate::spellcheck::SpellOfferPayload::empty(),
            );
        }
    }

    fn refresh_composing(line: &str) {
        let pending = PENDING.lock().map(|g| g.is_some()).unwrap_or(false);
        let on = ARMED.load(Ordering::Relaxed) && (!line.is_empty() || pending);
        COMPOSING.store(on, Ordering::Relaxed);
    }

    fn emit_digit_offer(app: &AppHandle, digits: &str) {
        let hwnd = ACTIVE_HWND.load(Ordering::Relaxed);
        crate::digit_actions::set_digit_target(hwnd, digits);
        let _ = app.emit(
            "mote://digit-offer",
            crate::watcher::DigitOfferPayload {
                digits: digits.to_string(),
            },
        );
    }

    fn clear_digit_offer(app: &AppHandle) {
        crate::digit_actions::clear_digit_target();
        let _ = app.emit(
            "mote://digit-offer",
            crate::watcher::DigitOfferPayload {
                digits: String::new(),
            },
        );
    }

    fn trailing_digit_run(line: &str) -> Option<String> {
        let n = line
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if n == 0 || n > 12 {
            return None;
        }
        let skip = line.chars().count().saturating_sub(n);
        let digits: String = line.chars().skip(skip).collect();
        // Isolated run: start of line or after a separator.
        let ok = skip == 0
            || line
                .chars()
                .nth(skip.saturating_sub(1))
                .is_some_and(is_separator);
        ok.then_some(digits)
    }

    fn sync_digit_from_line(app: &AppHandle, line: &str) {
        if let Some(digits) = trailing_digit_run(line) {
            emit_digit_offer(app, &digits);
        } else {
            clear_digit_offer(app);
        }
    }

    fn is_separator(c: char) -> bool {
        c.is_whitespace()
            || (c.is_ascii_punctuation() && c != '-' && c != '\'')
            || "«»…—–".contains(c)
    }

    fn take_after_replace() -> Option<String> {
        AFTER_REPLACE.lock().ok().and_then(|mut g| g.take())
    }

    pub fn start(app: AppHandle) {
        let (tx, rx) = mpsc::channel::<Ev>();
        if TX.set(Mutex::new(tx)).is_err() {
            return;
        }
        start_hook_thread();
        let _ = thread::Builder::new()
            .name("mote-canvas-worker".into())
            .spawn(move || {
                let my_pid = unsafe { GetCurrentProcessId() };
                let mut line = String::new();
                let mut last_pid = 0u32;
                let mut was_active = false;
                let mut mouse_was_down = false;
                let mut last_edit = Instant::now();
                let mut idle_checked = false;
                loop {
                    if let Some(fixed) = take_after_replace() {
                        line = fixed;
                        idle_checked = true;
                        last_edit = Instant::now();
                        refresh_composing(&line);
                    }

                    let ev = match rx.recv_timeout(Duration::from_millis(60)) {
                        Ok(ev) => Some(ev),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };

                    let fg = unsafe { GetForegroundWindow() };
                    let mut pid = 0u32;
                    unsafe {
                        GetWindowThreadProcessId(fg, Some(&mut pid));
                    }
                    if pid != 0 && pid != last_pid {
                        if pid != my_pid {
                            let name = proc_name(pid);
                            let active = ENABLED.load(Ordering::Relaxed)
                                && CANVAS_ALLOWLIST.contains(&name.as_str());
                            if last_pid != 0 {
                                line.clear();
                                idle_checked = false;
                                ARMED.store(false, Ordering::Relaxed);
                                clear_offer(&app, "foreground changed");
                                clear_digit_offer(&app);
                                refresh_composing(&line);
                            }
                            if active {
                                // Drop DigitTracker target from the previous app.
                                crate::digit_actions::clear_digit_target();
                                eprintln!("[mote] canvas: active proc={name}");
                            } else if was_active {
                                eprintln!("[mote] canvas: inactive (proc={name})");
                                ARMED.store(false, Ordering::Relaxed);
                                COMPOSING.store(false, Ordering::Relaxed);
                            }
                            ACTIVE.store(active, Ordering::Relaxed);
                            was_active = active;
                            last_pid = pid;
                        } else {
                            // Mote itself (plaque click): keep word/offer, stop capturing.
                            ACTIVE.store(false, Ordering::Relaxed);
                        }
                    } else if pid == last_pid && was_active {
                        ACTIVE.store(ENABLED.load(Ordering::Relaxed), Ordering::Relaxed);
                    }
                    if ACTIVE.load(Ordering::Relaxed) {
                        ACTIVE_HWND.store(fg.0 as isize, Ordering::Relaxed);
                    }

                    if was_active {
                        let down =
                            vk_down(VK_LBUTTON) || vk_down(VK_RBUTTON) || vk_down(VK_MBUTTON);
                        if down && !mouse_was_down {
                            let mut pt = POINT::default();
                            let on_mote = unsafe {
                                GetCursorPos(&mut pt).is_ok()
                                    && GetAncestor(WindowFromPoint(pt), GA_ROOT).0 as isize
                                        == mote_hwnd(&app)
                            };
                            if !on_mote {
                                // Click on canvas/UI arms typing (Text tool place / focus).
                                line.clear();
                                idle_checked = false;
                                clear_offer(&app, "mouse click");
                                clear_digit_offer(&app);
                                ARMED.store(true, Ordering::Relaxed);
                                refresh_composing(&line);
                                eprintln!("[mote] canvas: typing armed (click)");
                            }
                        }
                        mouse_was_down = down;
                    }

                    let Some(ev) = ev else {
                        // Re-check the whole line so earlier unconfirmed typos stay in the plaque.
                        if ARMED.load(Ordering::Relaxed)
                            && !idle_checked
                            && !line.is_empty()
                            && line
                                .chars()
                                .any(|c| !c.is_ascii_digit() && !c.is_whitespace())
                            && last_edit.elapsed() >= Duration::from_millis(750)
                        {
                            idle_checked = true;
                            clear_digit_offer(&app);
                            check_line(&app, &line);
                        }
                        refresh_composing(&line);
                        continue;
                    };
                    // Tool shortcuts / random keys before a canvas click — ignore.
                    if !ARMED.load(Ordering::Relaxed) {
                        continue;
                    }
                    match ev {
                        Ev::Reset(why) => {
                            line.clear();
                            idle_checked = false;
                            clear_offer(&app, why);
                            clear_digit_offer(&app);
                            refresh_composing(&line);
                        }
                        Ev::Back => {
                            line.pop();
                            last_edit = Instant::now();
                            idle_checked = false;
                            sync_digit_from_line(&app, &line);
                            refresh_composing(&line);
                        }
                        Ev::Char(c) => {
                            if line.chars().count() < MAX_LINE {
                                line.push(c);
                            }
                            last_edit = Instant::now();
                            idle_checked = false;
                            eprintln!("[mote] canvas: buf len={}", line.chars().count());
                            sync_digit_from_line(&app, &line);
                            if is_separator(c) {
                                idle_checked = true;
                                clear_digit_offer(&app);
                                check_line(&app, &line);
                            }
                            refresh_composing(&line);
                        }
                    }
                }
            });
    }

    fn check_line(app: &AppHandle, line: &str) {
        let len = line.chars().count();
        eprintln!("[mote] canvas: line len={len} checked");
        if line
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace())
        {
            return;
        }
        match crate::spellcheck::spell_offer(app.clone(), line.to_string()) {
            Ok(Some(offer)) => {
                let correction = offer.replaced.clone();
                if correction.is_empty() || correction == line {
                    clear_offer(app, "no change");
                    return;
                }
                let hwnd = ACTIVE_HWND.load(Ordering::Relaxed);
                if let Ok(mut g) = PENDING.lock() {
                    *g = Some(Pending {
                        hwnd,
                        typed_len: len,
                        correction,
                    });
                }
                let typos = offer.fixes.len();
                eprintln!("[mote] canvas: offer (line len={len}, typos={typos})");
                let _ = app.emit("mote://spell-offer", offer);
            }
            Ok(None) => {
                clear_offer(app, "line ok");
            }
            Err(e) => eprintln!("[mote] canvas: spellcheck error: {e}"),
        }
    }

    fn input(vk: VIRTUAL_KEY, scan: u16, unicode: bool, up: bool) -> INPUT {
        let mut flags = if unicode {
            KEYEVENTF_UNICODE
        } else {
            Default::default()
        };
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
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
        }
    }

    fn send_inputs(v: &[INPUT]) -> bool {
        let n = unsafe { SendInput(v, std::mem::size_of::<INPUT>() as i32) };
        n as usize == v.len()
    }

    pub fn try_replace() -> Option<Result<(), String>> {
        let p = PENDING.lock().ok()?.clone()?;
        Some(replace(&p))
    }

    fn replace(p: &Pending) -> Result<(), String> {
        let res = do_replace(p);
        match &res {
            Ok(()) => {
                eprintln!("[mote] canvas: replace ok");
                if let Ok(mut g) = PENDING.lock() {
                    *g = None;
                }
                if let Ok(mut g) = AFTER_REPLACE.lock() {
                    *g = Some(p.correction.clone());
                }
            }
            Err(e) => eprintln!("[mote] canvas: replace err: {e}"),
        }
        res
    }

    fn do_replace(p: &Pending) -> Result<(), String> {
        if p.hwnd == 0 {
            return Err("нет окна холста".into());
        }
        let _ = crate::digit_actions::set_clipboard(&p.correction);
        if !crate::digit_actions::focus_window(p.hwnd) {
            return Err("не удалось вернуть фокус в окно".into());
        }
        thread::sleep(Duration::from_millis(120));
        let fg = unsafe { GetForegroundWindow() };
        let root = |h: HWND| unsafe { GetAncestor(h, GA_ROOT).0 as isize };
        let target = HWND(p.hwnd as *mut core::ffi::c_void);
        if fg.0 as isize != p.hwnd && root(fg) != root(target) {
            return Err("окно холста не на переднем плане — ввод отменён".into());
        }
        let backs = p.typed_len;
        let mut v = Vec::with_capacity(backs * 2);
        for _ in 0..backs {
            v.push(input(VK_BACK, 0, false, false));
            v.push(input(VK_BACK, 0, false, true));
        }
        if !send_inputs(&v) {
            return Err("SendInput (Backspace) заблокирован".into());
        }
        thread::sleep(Duration::from_millis(40));
        let mut v = Vec::new();
        for u in p.correction.encode_utf16() {
            v.push(input(VIRTUAL_KEY(0), u, true, false));
            v.push(input(VIRTUAL_KEY(0), u, true, true));
        }
        if !send_inputs(&v) {
            return Err("SendInput (текст) заблокирован".into());
        }
        Ok(())
    }
}
