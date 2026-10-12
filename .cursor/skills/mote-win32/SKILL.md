---
name: mote-win32
description: Используй при работе с UI Automation, фокусом окон, буфером обмена, SendInput, хоткеями, WinRT (OCR, уведомления), захватом экрана.
---
- UIA: брать focused element → искать fill-control (Edit / ComboBox / Document). ValuePattern приоритетно,
  TextPattern только у focused-элемента. Никогда не брать текст всей страницы.
- Win11 Notepad: класс RichEditD2DPT, EM_REPLACESEL не работает → UIA или Ctrl+V.
- Electron/Chromium: дерево доступности ленивое; учитывать задержку.
- Возврат фокуса: AttachThreadInput(свой поток, поток цели) → SetForegroundWindow(top_hwnd) → отсоединить
  → пауза ~50 мс → SendInput. Писать по сохранённому HWND, не по текущему фокусу.
- Буфер обмена: Win32 OpenClipboard с повторами (занят другим процессом), сохранять и восстанавливать прежнее
  содержимое при вставке через Ctrl+V.
- OCR: Windows.Media.Ocr (язык ru). Захват экрана: xcap или windows-capture.
- Всё под #[cfg(windows)], ошибки логировать с HRESULT.
