//! Read / write the focused text field via UI Automation (actual control contents).

#[derive(Clone, Debug)]
pub struct FieldSnapshot {
    pub text: String,
    pub hwnd: isize,
    /// UIA control type debug name (Edit, Document, …) for diagnostics.
    pub control_type: String,
}

/// Screen rect of the caret (preferred) or the focused edit field.
#[derive(Clone, Copy, Debug)]
pub struct FieldRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub hwnd: isize,
}

/// Snapshot of the focused edit/document value, with a short miss reason for `[mote]` logs.
pub fn read_focused_field_detailed() -> Result<FieldSnapshot, &'static str> {
    #[cfg(windows)]
    {
        // Chromium/Electron often expose the a11y tree a tick late after focus/type.
        let mut last = win::read_focused_field_detailed();
        if last.is_ok() {
            return last;
        }
        for _ in 0..2 {
            if !matches!(last, Err("not input") | Err("unreadable") | Err("no focus")) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(45));
            last = win::read_focused_field_detailed();
            if last.is_ok() {
                return last;
            }
        }
        last
    }
    #[cfg(not(windows))]
    {
        Err("not windows")
    }
}

/// Single UIA attempt — for digit sync on the watcher thread (no sleep/retry).
pub fn read_focused_field_once() -> Result<FieldSnapshot, &'static str> {
    #[cfg(windows)]
    {
        win::read_focused_field_detailed()
    }
    #[cfg(not(windows))]
    {
        Err("not windows")
    }
}

/// One-line UIA focus dump for miss logs (Word / Notepad / Chrome).
pub fn focus_diag_line() -> String {
    #[cfg(windows)]
    {
        win::focus_diag_line()
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

/// Caret bar or typing field bounds — never the free mouse cursor.
pub fn focused_input_rect() -> Option<FieldRect> {
    #[cfg(windows)]
    {
        win::focused_input_rect()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Replace the entire focused field value (preferred over backspace+paste).
pub fn write_focused_field(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let text = text.to_string();
        win::on_sta_thread(move || win::write_focused_field_inner(&text))
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err("только Windows".into())
    }
}

/// Write by stored HWND (works even when Mote stole foreground focus).
pub fn write_field_hwnd(hwnd: isize, text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Tauri invoke threads are often MTA; uiautomation needs STA.
        let text = text.to_string();
        win::on_sta_thread(move || win::write_field_hwnd_inner(hwnd, &text))
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, text);
        Err("только Windows".into())
    }
}

/// Read field text from a known HWND (not whatever is focused now).
pub fn read_field_hwnd(hwnd: isize) -> Option<String> {
    #[cfg(windows)]
    {
        win::on_sta_thread(move || Ok(win::read_field_hwnd_inner(hwnd)))
            .ok()
            .flatten()
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        None
    }
}

/// Focused-field read for verify after replace (STA thread — safe from invoke).
pub fn read_focused_field_sta() -> Option<FieldSnapshot> {
    #[cfg(windows)]
    {
        win::on_sta_thread(|| win::read_focused_field_detailed().map_err(|e| e.to_string())).ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Currently selected text in the focused field (on-demand spell), if any.
pub fn read_selection() -> Result<FieldSnapshot, String> {
    #[cfg(windows)]
    {
        win::on_sta_thread(|| win::read_selection_inner())
    }
    #[cfg(not(windows))]
    {
        Err("только Windows".into())
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::mpsc;
    use uiautomation::patterns::{UITextPattern, UIValuePattern};
    use uiautomation::types::{ControlType, UIProperty};
    use uiautomation::UIAutomation;
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetParent, GetWindow, GetWindowLongW,
        GetWindowRect, IsWindow, IsWindowVisible, SendMessageW, GUITHREADINFO, GWL_STYLE, GW_CHILD,
        GW_HWNDNEXT, WM_GETTEXT, WM_GETTEXTLENGTH,
    };

    /// Fresh thread for UIA from Tauri invoke: `UIAutomation::new()` calls
    /// `CoInitializeEx(MTA)` and errors with RPC_E_CHANGED_MODE if the thread
    /// already has a different apartment (common on async/runtime threads).
    pub fn on_sta_thread<R, F>(f: F) -> Result<R, String>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, String> + Send + 'static,
    {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("mote-uia".into())
            .spawn(move || {
                // Do not CoInitialize here — UIAutomation::new() sets MTA itself.
                let _ = tx.send(f());
            })
            .map_err(|e| format!("uia spawn: {e}"))?;
        rx.recv().map_err(|e| format!("uia thread: {e}"))?
    }

    /// Prefer `new()` (inits MTA); if apartment already set, use `new_direct()`.
    fn create_uia() -> Result<UIAutomation, String> {
        match UIAutomation::new() {
            Ok(u) => Ok(u),
            Err(_) => UIAutomation::new_direct().map_err(|e| format!("UIA: {e}")),
        }
    }

    /// Soft cap: form fields / chat boxes, not whole documents.
    const MAX_FIELD_CHARS: usize = 1_200;
    const MAX_FIELD_LINES: usize = 40;

    fn hwnd_from_element(el: &uiautomation::UIElement) -> isize {
        el.get_native_window_handle()
            .ok()
            .map(|h| h.into())
            .unwrap_or(0)
    }

    /// Electron/Chromium often expose Edit with native HWND=0 — walk parents.
    fn hwnd_from_element_chain(automation: &UIAutomation, el: &uiautomation::UIElement) -> isize {
        let direct = hwnd_from_element(el);
        if direct != 0 {
            return direct;
        }
        let Ok(walker) = automation.get_control_view_walker() else {
            return 0;
        };
        let mut cur = el.clone();
        for _ in 0..10 {
            match walker.get_parent(&cur) {
                Ok(parent) => {
                    let h = hwnd_from_element(&parent);
                    if h != 0 {
                        return h;
                    }
                    cur = parent;
                }
                Err(_) => break,
            }
        }
        0
    }

    /// Last resort when UIA NativeWindowHandle is 0 on the whole chain (Chromium).
    fn hwnd_from_win32_focus() -> isize {
        unsafe {
            let mut gi = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(0, &mut gi).is_ok() && !gi.hwndFocus.0.is_null() {
                return gi.hwndFocus.0 as isize;
            }
            let fg = GetForegroundWindow();
            if !fg.0.is_null() {
                return fg.0 as isize;
            }
        }
        0
    }

    fn normalize_field_text(s: &str) -> String {
        s.trim_end_matches('\0').to_string()
    }

    fn within_field_budget(s: &str) -> bool {
        let chars = s.chars().count();
        chars <= MAX_FIELD_CHARS && s.lines().count() <= MAX_FIELD_LINES
    }

    fn class_name_of(el: &uiautomation::UIElement) -> String {
        el.get_classname().unwrap_or_default()
    }

    fn win32_class_name(hwnd: isize) -> String {
        if hwnd == 0 {
            return String::new();
        }
        let mut buf = [0u16; 256];
        let n = unsafe { GetClassNameW(HWND(hwnd as *mut core::ffi::c_void), &mut buf) };
        if n == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }

    /// Win11 Notepad editor (and classic RichEdit) — often tall Document without ValuePattern.
    fn is_richedit_editor(el: &uiautomation::UIElement) -> bool {
        let uia = class_name_of(el);
        if uia.contains("RichEditD2DPT") || uia.contains("RichEdit") {
            return true;
        }
        let hwnd = hwnd_from_element(el);
        let win = win32_class_name(hwnd);
        win.contains("RichEditD2DPT") || win.contains("RichEdit")
    }

    /// Microsoft Word document surface (`_WwG`) — tall Document, TextPattern, no ValuePattern.
    fn is_word_document(el: &uiautomation::UIElement) -> bool {
        let uia = class_name_of(el);
        if uia.contains("_WwG") {
            return true;
        }
        let hwnd = hwnd_from_element(el);
        win32_class_name(hwnd).contains("_WwG")
    }

    fn is_large_text_surface(el: &uiautomation::UIElement) -> bool {
        is_richedit_editor(el) || is_word_document(el)
    }

    fn looks_like_input_rect(el: &uiautomation::UIElement) -> bool {
        let Ok(r) = el.get_bounding_rectangle() else {
            return false;
        };
        let w = r.get_right() - r.get_left();
        let h = r.get_bottom() - r.get_top();
        // Chat / single-line: short. Notepad / Word: allow taller surface.
        let (max_w, max_h) = if is_large_text_surface(el) {
            (2800, 1600)
        } else {
            (2400, 480)
        };
        w >= 32 && h >= 12 && w <= max_w && h <= max_h
    }

    fn control_type_of(el: &uiautomation::UIElement) -> Option<ControlType> {
        el.get_control_type().ok()
    }

    fn is_input_control_type(ct: ControlType) -> bool {
        matches!(
            ct,
            ControlType::Edit
                | ControlType::ComboBox
                | ControlType::Document
                | ControlType::Custom
                | ControlType::DataItem
                | ControlType::Spinner
        )
    }

    fn has_value_pattern(el: &uiautomation::UIElement) -> bool {
        el.get_pattern::<UIValuePattern>().is_ok()
    }

    /// True for a focused fill/edit control — not static Text / whole window.
    fn is_fill_field(el: &uiautomation::UIElement) -> bool {
        let Some(ct) = control_type_of(el) else {
            return has_value_pattern(el) && looks_like_input_rect(el);
        };
        // Static labels / page chrome — never.
        if matches!(
            ct,
            ControlType::Text | ControlType::Window | ControlType::Pane
        ) {
            return false;
        }
        if !is_input_control_type(ct) {
            return false;
        }
        match ct {
            ControlType::Edit | ControlType::ComboBox | ControlType::Spinner => true,
            ControlType::Document => {
                // Notepad RichEdit / Word _WwG: tall Document, often no ValuePattern.
                // Web pages: huge Document without those classes — rejected by rect.
                is_large_text_surface(el) || looks_like_input_rect(el) || has_value_pattern(el)
            }
            _ => has_value_pattern(el) && looks_like_input_rect(el),
        }
    }

    fn value_nonempty(el: &uiautomation::UIElement) -> Option<String> {
        if let Ok(pattern) = el.get_pattern::<UIValuePattern>() {
            if let Ok(s) = pattern.get_value() {
                let s = normalize_field_text(&s);
                if !s.trim().is_empty() && within_field_budget(&s) {
                    return Some(s);
                }
            }
        }
        if let Ok(val) = el.get_property_value(UIProperty::ValueValue) {
            if let Ok(s) = val.get_string() {
                let s = normalize_field_text(&s);
                if !s.trim().is_empty() && within_field_budget(&s) {
                    return Some(s);
                }
            }
        }
        None
    }

    fn text_pattern_nonempty(el: &uiautomation::UIElement) -> Option<String> {
        let pattern = el.get_pattern::<UITextPattern>().ok()?;
        let range = pattern.get_document_range().ok()?;
        let s = range.get_text(-1).ok()?;
        let s = normalize_field_text(&s);
        let budget_ok = if is_large_text_surface(el) {
            s.chars().count() <= 8_000 && s.lines().count() <= 400
        } else {
            within_field_budget(&s)
        };
        if !s.trim().is_empty() && budget_ok {
            Some(s)
        } else {
            None
        }
    }

    /// Value of the input only — never the longest TextPattern dump on screen.
    fn read_input_value(el: &uiautomation::UIElement) -> Option<String> {
        // Prefer non-empty Value* (Chromium Edit). Skip empty Value — Word/TipTap
        // expose text only via TextPattern on the same element.
        if let Some(s) = value_nonempty(el) {
            return Some(s);
        }
        text_pattern_nonempty(el)
    }

    fn find_fill_among_children(
        automation: &UIAutomation,
        el: &uiautomation::UIElement,
        depth: u32,
    ) -> Option<uiautomation::UIElement> {
        if depth == 0 {
            return None;
        }
        let walker = automation.get_control_view_walker().ok()?;
        let Ok(mut child) = walker.get_first_child(el) else {
            return None;
        };
        for _ in 0..16 {
            if is_fill_field(&child) {
                // Require readable text — empty Value Edit (Word page chrome) must not win.
                if read_input_value(&child).is_some() {
                    return Some(child);
                }
            }
            if let Some(found) = find_fill_among_children(automation, &child, depth - 1) {
                return Some(found);
            }
            match walker.get_next_sibling(&child) {
                Ok(next) => child = next,
                Err(_) => break,
            }
        }
        None
    }

    /// Walk focus → parents → children to the actual fill control.
    fn resolve_fill_field(
        automation: &UIAutomation,
        focused: &uiautomation::UIElement,
    ) -> Option<uiautomation::UIElement> {
        // Word / Notepad Document: use the focused surface (TextPattern), not a child Edit.
        if is_word_document(focused) || is_richedit_editor(focused) {
            if control_type_of(focused) == Some(ControlType::Document)
                || read_input_value(focused).is_some()
            {
                return Some(focused.clone());
            }
        }
        if is_fill_field(focused) {
            if read_input_value(focused).is_some() {
                return Some(focused.clone());
            }
            // Empty Edit (ribbon / page stub) — keep searching for Document body.
        }
        if let Ok(walker) = automation.get_control_view_walker() {
            let mut cur = focused.clone();
            for _ in 0..6 {
                match walker.get_parent(&cur) {
                    Ok(parent) => {
                        if is_word_document(&parent) || is_richedit_editor(&parent) {
                            return Some(parent);
                        }
                        if is_fill_field(&parent) && read_input_value(&parent).is_some() {
                            return Some(parent);
                        }
                        cur = parent;
                    }
                    Err(_) => break,
                }
            }
        }
        // WinForms often focuses the form Pane; real TextBox may be nested deeper.
        let child_depth = if matches!(control_type_of(focused), Some(ControlType::Pane))
            && class_name_of(focused).contains("WindowsForms10")
        {
            5
        } else {
            2
        };
        find_fill_among_children(automation, focused, child_depth)
    }

    fn element_field_rect(el: &uiautomation::UIElement) -> Option<FieldRect> {
        let r = el.get_bounding_rectangle().ok()?;
        let left = r.get_left();
        let top = r.get_top();
        let right = r.get_right();
        let bottom = r.get_bottom();
        if right - left < 4 || bottom - top < 4 {
            return None;
        }
        // Skip full-screen surfaces, but allow wide editor/chat inputs.
        if right - left > 2400 || bottom - top > 900 {
            return None;
        }
        Some(FieldRect {
            left,
            top,
            right,
            bottom,
            hwnd: hwnd_from_element(el),
        })
    }

    /// Estimate caret including soft-wrap and hard newlines (`\\n` / `\\r\\n`).
    fn caret_from_text(field: FieldRect, text: &str) -> FieldRect {
        let w = (field.right - field.left).max(1);
        let h = (field.bottom - field.top).max(1);
        let pad = 8;
        let avg_char = 8i32;
        let chars_per_line = ((w - pad * 2) / avg_char).max(8) as usize;

        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut visual_rows = 0usize;
        let mut last_cols = 0usize;
        // Trailing newline = caret on a fresh empty paragraph at column 0.
        let ends_with_break = normalized.ends_with('\n');
        for para in normalized.split('\n') {
            let len = para.chars().count();
            if len == 0 {
                visual_rows += 1;
                last_cols = 0;
            } else {
                let rows = (len + chars_per_line - 1) / chars_per_line;
                visual_rows += rows.max(1);
                last_cols = len % chars_per_line;
                if last_cols == 0 {
                    last_cols = chars_per_line;
                }
            }
        }
        if ends_with_break {
            // split already counted the empty trailing para; keep col 0.
            last_cols = 0;
        }
        if visual_rows == 0 {
            visual_rows = 1;
        }

        let line_h = 22i32.min((h - pad).max(14));
        // While typing at the end of a tall box, assume the view is scrolled to the caret.
        let top = if (visual_rows as i32) * line_h > h - pad {
            (field.bottom - pad - line_h).max(field.top + pad)
        } else {
            (field.top + pad + (visual_rows as i32 - 1) * line_h).min(field.bottom - line_h)
        };
        let bottom = (top + line_h).min(field.bottom - 2);
        let approx_x = field.left + pad + ((last_cols as i32) * avg_char).min(w - pad * 2).max(0);
        FieldRect {
            left: approx_x,
            top: top.max(field.top),
            right: (approx_x + 2).min(field.right),
            bottom: bottom.max(top + 12),
            hwnd: field.hwnd,
        }
    }

    fn lineish_caret_rect(
        el: &uiautomation::UIElement,
        enc: &uiautomation::UIElement,
    ) -> Option<FieldRect> {
        let r = enc.get_bounding_rectangle().ok()?;
        let left = r.get_left();
        let top = r.get_top();
        let right = r.get_right();
        let bottom = r.get_bottom();
        let h = bottom - top;
        let w = right - left;
        if h < 8 || h > 56 || w < 1 {
            return None;
        }
        // Line/glyph box: pin eyes to the left edge (caret), not the whole wrap width.
        Some(FieldRect {
            left,
            top,
            right: left + 2,
            bottom: bottom.max(top + 12),
            hwnd: hwnd_from_element(el),
        })
    }

    fn try_caret_via_text_pattern(el: &uiautomation::UIElement) -> Option<FieldRect> {
        let pattern = el.get_pattern::<UITextPattern>().ok()?;
        if let Ok((_active, range)) = pattern.get_caret_range() {
            if let Ok(enc) = range.get_enclosing_element() {
                if let Some(r) = lineish_caret_rect(el, &enc) {
                    return Some(r);
                }
            }
        }
        if let Ok(sels) = pattern.get_selection() {
            if let Some(sel) = sels.first() {
                if let Ok(enc) = sel.get_enclosing_element() {
                    if let Some(r) = lineish_caret_rect(el, &enc) {
                        return Some(r);
                    }
                }
            }
        }
        None
    }

    pub fn focused_input_rect() -> Option<FieldRect> {
        let automation = UIAutomation::new().ok()?;
        let focused = automation.get_focused_element().ok()?;
        let el = resolve_fill_field(&automation, &focused).unwrap_or(focused);
        let hwnd = hwnd_from_element(&el);

        if let Some(mut caret) = try_caret_via_text_pattern(&el) {
            if caret.hwnd == 0 {
                caret.hwnd = hwnd;
            }
            return Some(caret);
        }

        let field = element_field_rect(&el)?;
        if let Some(text) = read_input_value(&el) {
            return Some(caret_from_text(field, &text));
        }
        // Field center-right — enough to look at the input without chasing glyphs.
        let mid_y = (field.top + field.bottom) / 2;
        Some(FieldRect {
            left: field.left + (field.right - field.left) / 3,
            top: mid_y - 6,
            right: field.left + (field.right - field.left) / 3 + 2,
            bottom: mid_y + 6,
            hwnd: field.hwnd,
        })
    }

    fn control_type_name(el: &uiautomation::UIElement) -> String {
        let ct = control_type_of(el)
            .map(|ct| format!("{ct:?}"))
            .unwrap_or_else(|| "Unknown".into());
        let class = class_name_of(el);
        if class.is_empty() {
            ct
        } else {
            format!("{ct}/{class}")
        }
    }

    fn short_name(el: &uiautomation::UIElement) -> String {
        let n = el.get_name().unwrap_or_default();
        let mut it = n.chars();
        let head: String = it.by_ref().take(48).collect();
        if it.next().is_some() {
            format!("{head}…")
        } else {
            head
        }
    }

    fn describe_el(el: &uiautomation::UIElement) -> String {
        let rect = el
            .get_bounding_rectangle()
            .ok()
            .map(|r| {
                format!(
                    "{}x{}",
                    r.get_right() - r.get_left(),
                    r.get_bottom() - r.get_top()
                )
            })
            .unwrap_or_else(|| "?".into());
        let val = has_value_pattern(el);
        let text_pat = el.get_pattern::<UITextPattern>().is_ok();
        format!(
            "{} name=\"{}\" rect={} val={} textPat={} winClass={}",
            control_type_name(el),
            short_name(el),
            rect,
            val,
            text_pat,
            win32_class_name(hwnd_from_element(el))
        )
    }

    fn gui_focus_info() -> Option<GUITHREADINFO> {
        unsafe {
            let mut gi = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            GetGUIThreadInfo(0, &mut gi).ok()?;
            Some(gi)
        }
    }

    /// Classic / WinForms Edit — grok patch used `== "edit"` and missed `WindowsForms10.EDIT`.
    fn is_win32_edit_class(class: &str) -> bool {
        let lc = class.to_ascii_lowercase();
        lc == "edit"
            || lc.starts_with("richedit")
            || lc.contains("richedit")
            || lc.contains(".edit")
            || lc.contains("textbox")
    }

    const ES_READONLY: i32 = 0x0800;

    fn read_text_from_hwnd(hwnd: HWND) -> Option<String> {
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return None;
            }
            if GetWindowLongW(hwnd, GWL_STYLE) & ES_READONLY != 0 {
                return None;
            }
            let len = SendMessageW(hwnd, WM_GETTEXTLENGTH, None, None).0;
            if len <= 0 || len as usize > MAX_FIELD_CHARS * 2 {
                return None;
            }
            let mut buf = vec![0u16; len as usize + 1];
            let got = SendMessageW(
                hwnd,
                WM_GETTEXT,
                Some(WPARAM(buf.len())),
                Some(LPARAM(buf.as_mut_ptr() as isize)),
            )
            .0;
            if got <= 0 {
                return None;
            }
            let s = normalize_field_text(&String::from_utf16_lossy(&buf[..got as usize]));
            if s.trim().is_empty() || !within_field_budget(&s) {
                return None;
            }
            Some(s)
        }
    }

    fn collect_edit_hwnds(root: HWND, out: &mut Vec<HWND>, depth: u32) {
        if depth == 0 || root.0.is_null() {
            return;
        }
        unsafe {
            let Ok(mut child) = GetWindow(root, GW_CHILD) else {
                return;
            };
            while !child.0.is_null() {
                if IsWindowVisible(child).as_bool() {
                    let class = win32_class_name(child.0 as isize);
                    if is_win32_edit_class(&class) {
                        out.push(child);
                    }
                    collect_edit_hwnds(child, out, depth - 1);
                }
                child = match GetWindow(child, GW_HWNDNEXT) {
                    Ok(next) => next,
                    Err(_) => break,
                };
            }
        }
    }

    fn hwnd_screen_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let mut rc = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rc).ok()?;
            Some((rc.left, rc.top, rc.right, rc.bottom))
        }
    }

    fn caret_screen_point(gi: &GUITHREADINFO) -> Option<(i32, i32)> {
        if gi.hwndCaret.0.is_null() {
            return None;
        }
        let rc = gi.rcCaret;
        let mut pt = POINT {
            x: rc.left,
            y: (rc.top + rc.bottom) / 2,
        };
        unsafe {
            if !ClientToScreen(gi.hwndCaret, &mut pt).as_bool() {
                return None;
            }
        }
        Some((pt.x, pt.y))
    }

    fn rect_contains(hwnd: HWND, x: i32, y: i32) -> bool {
        let Some((l, t, r, b)) = hwnd_screen_rect(hwnd) else {
            return false;
        };
        x >= l && x < r && y >= t && y < b
    }

    /// Width/height/zoom boxes — not the document being typed.
    fn is_toolbar_edit(hwnd: HWND) -> bool {
        let Some((l, t, r, b)) = hwnd_screen_rect(hwnd) else {
            return true;
        };
        let w = r - l;
        let h = b - t;
        h < 28 || w < 56 || h > 120 && w < 200
    }

    fn snap_from_edit_hwnd(hwnd: HWND) -> Option<FieldSnapshot> {
        let class = win32_class_name(hwnd.0 as isize);
        let text = read_text_from_hwnd(hwnd)?;
        Some(FieldSnapshot {
            text,
            hwnd: hwnd.0 as isize,
            control_type: format!("Win32/{class}"),
        })
    }

    /// WinForms dialogs: Edit under focus. Never scoop a random toolbar Edit under the form
    /// (Paint.NET had len=2 width/zoom while Text tool text is on the canvas — no HWND).
    fn find_win32_edit_snapshot() -> Option<FieldSnapshot> {
        let gi = gui_focus_info()?;
        let caret = caret_screen_point(&gi);

        // Direct focus / caret Edit only if it looks like a real text box.
        for h in [gi.hwndFocus, gi.hwndCaret] {
            if h.0.is_null() || !is_win32_edit_class(&win32_class_name(h.0 as isize)) {
                continue;
            }
            if is_toolbar_edit(h) {
                continue;
            }
            if let Some(snap) = snap_from_edit_hwnd(h) {
                return Some(snap);
            }
        }

        // Otherwise only an Edit that contains the caret — not "longest under form".
        let (cx, cy) = caret?;
        let mut candidates: Vec<HWND> = Vec::new();
        let roots = [gi.hwndFocus, gi.hwndActive, unsafe {
            GetForegroundWindow()
        }];
        for root in roots {
            if root.0.is_null() {
                continue;
            }
            collect_edit_hwnds(root, &mut candidates, 6);
            if let Ok(parent) = unsafe { GetParent(root) } {
                if !parent.0.is_null() {
                    collect_edit_hwnds(parent, &mut candidates, 6);
                }
            }
        }
        let mut best: Option<(usize, FieldSnapshot)> = None;
        for hwnd in candidates {
            if !rect_contains(hwnd, cx, cy) || is_toolbar_edit(hwnd) {
                continue;
            }
            let Some(snap) = snap_from_edit_hwnd(hwnd) else {
                continue;
            };
            let len = snap.text.chars().count();
            if best.as_ref().map(|(s, _)| len > *s).unwrap_or(true) {
                best = Some((len, snap));
            }
        }
        best.map(|(_, s)| s)
    }

    pub fn focus_diag_line() -> String {
        let gi = gui_focus_info();
        let w32 = gi
            .map(|g| {
                format!(
                    "focusHwnd={} caretHwnd={} active={}",
                    win32_class_name(g.hwndFocus.0 as isize),
                    win32_class_name(g.hwndCaret.0 as isize),
                    win32_class_name(g.hwndActive.0 as isize)
                )
            })
            .unwrap_or_else(|| "gui=none".into());
        let win32_edit = find_win32_edit_snapshot()
            .map(|s| {
                format!(
                    "win32Edit={} len={}",
                    s.control_type,
                    s.text.chars().count()
                )
            })
            .unwrap_or_else(|| "win32Edit=none".into());

        let Ok(automation) = create_uia() else {
            return format!("{w32} | {win32_edit} | uia init failed");
        };
        let Ok(focused) = automation.get_focused_element() else {
            return format!("{w32} | {win32_edit} | no focus");
        };
        let fill = resolve_fill_field(&automation, &focused);
        match fill {
            Some(field) => format!(
                "{w32} | {win32_edit} | focus={} | fill={}",
                describe_el(&focused),
                describe_el(&field)
            ),
            None => format!(
                "{w32} | {win32_edit} | focus={} | fill=none",
                describe_el(&focused)
            ),
        }
    }

    pub fn read_focused_field_detailed() -> Result<FieldSnapshot, &'static str> {
        let uia = read_focused_field_uia();
        if uia.is_ok() || matches!(uia, Err("empty")) {
            return uia;
        }
        // WinForms dialogs: real Edit under caret (not toolbar noise under Paint.NET Pane).
        if let Some(snap) = find_win32_edit_snapshot() {
            return Ok(snap);
        }
        uia
    }

    pub fn read_selection_inner() -> Result<FieldSnapshot, String> {
        let automation = create_uia().map_err(|e| e.to_string())?;
        let focused = automation
            .get_focused_element()
            .map_err(|_| "нет фокуса".to_string())?;
        let field = resolve_fill_field(&automation, &focused).unwrap_or(focused);
        let pattern = field
            .get_pattern::<UITextPattern>()
            .map_err(|_| "нет TextPattern".to_string())?;
        let sels = pattern
            .get_selection()
            .map_err(|_| "нет выделения".to_string())?;
        let mut text = String::new();
        for sel in sels.iter() {
            if let Ok(chunk) = sel.get_text(-1) {
                text.push_str(&chunk);
            }
        }
        let text = normalize_field_text(&text);
        if text.trim().is_empty() {
            return Err("выдели текст".into());
        }
        if text.chars().count() > 8_000 {
            return Err("выделение слишком большое".into());
        }
        let mut hwnd = hwnd_from_element_chain(&automation, &field);
        if hwnd == 0 {
            hwnd = hwnd_from_win32_focus();
        }
        Ok(FieldSnapshot {
            text,
            hwnd,
            control_type: format!("Selection/{}", control_type_name(&field)),
        })
    }

    fn read_focused_field_uia() -> Result<FieldSnapshot, &'static str> {
        let automation = create_uia().map_err(|_| "uia init")?;
        let focused = automation.get_focused_element().map_err(|_| "no focus")?;
        let Some(field) = resolve_fill_field(&automation, &focused) else {
            return Err("not input");
        };
        let Some(text) = read_input_value(&field) else {
            return Err("unreadable");
        };
        if text.trim().is_empty() {
            return Err("empty");
        }
        // read_input_value already capped Word/RichEdit at 8k; don't re-reject with chat budget.
        let over_budget = if is_large_text_surface(&field) {
            text.chars().count() > 8_000 || text.lines().count() > 400
        } else {
            !within_field_budget(&text)
        };
        if over_budget {
            return Err("budget");
        }
        let mut hwnd = hwnd_from_element_chain(&automation, &field);
        if hwnd == 0 {
            hwnd = hwnd_from_element_chain(&automation, &focused);
        }
        if hwnd == 0 {
            hwnd = hwnd_from_win32_focus();
        }
        Ok(FieldSnapshot {
            text,
            hwnd,
            control_type: control_type_name(&field),
        })
    }

    pub fn read_field_hwnd_inner(hwnd: isize) -> Option<String> {
        if hwnd == 0 {
            return None;
        }
        let automation = create_uia().ok()?;
        let el = automation
            .element_from_handle(uiautomation::types::Handle::from(hwnd))
            .ok()?;
        let field = resolve_fill_field(&automation, &el).unwrap_or(el);
        let text = read_input_value(&field)?;
        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn try_set_value(el: &uiautomation::UIElement, text: &str) -> bool {
        let Ok(pattern) = el.get_pattern::<UIValuePattern>() else {
            return false;
        };
        let _ = el.set_focus();
        if pattern.set_value(text).is_err() {
            return false;
        }
        match pattern.get_value() {
            Ok(v) => {
                let v = v.trim_end_matches('\0');
                v == text || (!text.is_empty() && v.contains(text))
            }
            Err(_) => true,
        }
    }

    fn write_element_tree(el: &uiautomation::UIElement, text: &str, depth: u32) -> bool {
        if try_set_value(el, text) {
            return true;
        }
        if depth == 0 {
            return false;
        }
        let Ok(automation) = create_uia() else {
            return false;
        };
        let Ok(walker) = automation.get_control_view_walker() else {
            return false;
        };
        let Ok(mut child) = walker.get_first_child(el) else {
            return false;
        };
        for _ in 0..12 {
            if write_element_tree(&child, text, depth - 1) {
                return true;
            }
            match walker.get_next_sibling(&child) {
                Ok(next) => child = next,
                Err(_) => break,
            }
        }
        false
    }

    pub fn write_field_hwnd_inner(hwnd: isize, text: &str) -> Result<(), String> {
        if hwnd == 0 {
            return Err("нет hwnd поля".into());
        }
        let automation = create_uia()?;
        let el = automation
            .element_from_handle(uiautomation::types::Handle::from(hwnd))
            .map_err(|e| format!("element_from_handle: {e}"))?;
        let field = resolve_fill_field(&automation, &el).unwrap_or(el);
        if try_set_value(&field, text) || write_element_tree(&field, text, 2) {
            eprintln!("[mote] spell write ok via UIA hwnd={hwnd:#x}");
            return Ok(());
        }
        Err("UIA ValuePattern недоступен для hwnd".into())
    }

    pub fn write_focused_field_inner(text: &str) -> Result<(), String> {
        let automation = create_uia()?;
        let focused = automation
            .get_focused_element()
            .map_err(|e| format!("focus: {e}"))?;
        let field = resolve_fill_field(&automation, &focused).unwrap_or(focused);
        if try_set_value(&field, text) || write_element_tree(&field, text, 2) {
            eprintln!("[mote] spell write ok via focused UIA");
            return Ok(());
        }
        Err("поле не поддерживает прямую запись текста".into())
    }
}
