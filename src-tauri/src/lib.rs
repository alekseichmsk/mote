mod browser_url;
mod canvas_typing;
mod curious;
mod digit_actions;
mod field_text;
mod hit_regions;
mod layout_fix;
mod llm;
mod media_session;
mod selection_spell;
mod spell_ignore;
mod spellcheck;
mod update;
mod watcher;
mod weather;

use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = main_window(app) {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.set_always_on_top(true);
        }
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = main_window(app) {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.set_always_on_top(true);
    }
}

#[tauri::command]
fn show_main_window(app: AppHandle) {
    show_window(&app);
}

#[tauri::command]
fn hide_main_window(app: AppHandle) {
    if let Some(window) = main_window(&app) {
        let _ = window.hide();
    }
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    let window = main_window(&app).ok_or_else(|| "окно не найдено".to_string())?;
    window.set_always_on_top(enabled).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_presence_paused(paused: bool) {
    watcher::set_paused(paused);
}

#[tauri::command]
fn set_presence_enabled(enabled: bool) {
    watcher::set_enabled(enabled);
}

#[tauri::command]
fn mark_user_placed(duration_ms: u64) {
    watcher::mark_user_placed(duration_ms);
}

#[tauri::command]
fn shoo_away(app: AppHandle) -> Result<(), String> {
    watcher::shoo_away(&app)
}

#[tauri::command]
fn enter_rage_mode(app: AppHandle) -> Result<(), String> {
    watcher::enter_rage_mode(&app)
}

#[tauri::command]
fn exit_rage_mode(app: AppHandle) -> Result<(), String> {
    watcher::exit_rage_mode(&app)
}

#[tauri::command]
fn media_control(action: String) -> Result<(), String> {
    media_session::control(&action)
}

#[tauri::command]
fn media_volume_get() -> Result<f32, String> {
    media_session::get_volume()
}

#[tauri::command]
fn media_volume_set(level: f32) -> Result<(), String> {
    media_session::set_volume(level)
}

#[tauri::command]
fn digit_copy(words: String) -> Result<(), String> {
    eprintln!(
        "[mote] digit_copy invoked ({} chars)",
        words.chars().count()
    );
    digit_actions::copy_words(&words)
}

#[tauri::command]
fn digit_replace(words: String) -> Result<(), String> {
    eprintln!(
        "[mote] digit_replace invoked ({} chars)",
        words.chars().count()
    );
    digit_actions::replace_with_words(&words)
}

#[tauri::command]
fn set_hit_regions(regions: Vec<hit_regions::HitRect>, menu: Option<hit_regions::HitRect>) {
    hit_regions::set_regions(regions, menu);
}

#[tauri::command]
fn spell_preload(app: AppHandle) {
    spellcheck::preload(app);
}

#[tauri::command]
fn spell_ready(app: AppHandle) -> bool {
    spellcheck::spell_ready(app)
}

#[tauri::command]
fn spell_offer(
    app: AppHandle,
    text: String,
) -> Result<Option<spellcheck::SpellOfferPayload>, String> {
    spellcheck::spell_offer(app, text)
}

/// Prefer writing the full corrected field via UIA; fall back to select-all/paste.
#[tauri::command]
fn spell_replace(replaced: String, suggestion: String) -> Result<(), String> {
    eprintln!(
        "[mote] spell_replace invoked (replaced={} sug={})",
        replaced.chars().count(),
        suggestion.chars().count()
    );
    digit_actions::replace_spell(&replaced, &suggestion)
}

#[tauri::command]
fn spell_check_selection(app: AppHandle) -> Result<(), String> {
    selection_spell::check_selection(app)
}

#[tauri::command]
fn layout_fix(app: AppHandle, text: String, force: bool) -> Option<String> {
    layout_fix::layout_fix(&app, text, force)
}

#[tauri::command]
fn layout_auto_get() -> bool {
    layout_fix::is_auto_enabled()
}

#[tauri::command]
fn layout_auto_set(enabled: bool) -> Result<(), String> {
    layout_fix::set_auto_enabled(enabled)
}

#[tauri::command]
fn spell_clear_target() {
    digit_actions::clear_digit_target();
}

#[tauri::command]
fn spell_ignore_add(word: String) -> Result<usize, String> {
    spell_ignore::add_words(&word)
}

#[tauri::command]
fn llm_status() -> llm::LlmStatus {
    llm::status()
}

#[tauri::command]
fn llm_set_device(device: String) -> Result<(), String> {
    llm::set_device(llm::LlmDevice::from_str_loose(&device))
}

#[tauri::command]
fn llm_download_model(app: AppHandle) -> Result<(), String> {
    llm::download_model(app)
}

#[tauri::command]
async fn llm_improve_text(app: AppHandle, text: String) -> Result<llm::ImproveResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = llm::status();
        if !st.model_present {
            llm::download_model(app)?;
            return Err("DOWNLOADING".into());
        }
        if !st.runtime_present {
            llm::download_runtime(app)?;
            return Err("DOWNLOADING".into());
        }
        llm::improve_text(app, text)
    })
    .await
    .map_err(|e| format!("llm task: {e}"))?
}

#[tauri::command]
async fn llm_improve_selection(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = llm::status();
        if !st.model_present {
            llm::download_model(app)?;
            return Err("DOWNLOADING".into());
        }
        if !st.runtime_present {
            llm::download_runtime(app)?;
            return Err("DOWNLOADING".into());
        }
        selection_spell::improve_selection(app)
    })
    .await
    .map_err(|e| format!("llm task: {e}"))?
}

#[tauri::command]
fn weather_now(city: String) -> Result<String, String> {
    weather::weather_now(city)
}

#[tauri::command]
fn curious_number_fact(n: u64, kind: Option<String>) -> Result<curious::CuriousLine, String> {
    curious::number_fact(n, kind)
}

#[tauri::command]
fn curious_date_fact(month: u32, day: u32) -> Result<curious::CuriousLine, String> {
    curious::date_fact(month, day)
}

#[tauri::command]
fn curious_quote() -> Result<curious::CuriousLine, String> {
    curious::random_quote()
}

#[tauri::command]
fn curious_wiki(query: String) -> Result<curious::CuriousLine, String> {
    curious::wiki_summary(query)
}

#[tauri::command]
fn curious_wiki_random() -> Result<curious::CuriousLine, String> {
    curious::wiki_random()
}

#[tauri::command]
fn curious_country(name: String) -> Result<curious::CuriousLine, String> {
    curious::country_info(name)
}

#[tauri::command]
fn curious_book(query: String) -> Result<curious::CuriousLine, String> {
    curious::book_search(query)
}

#[tauri::command]
fn curious_nasa_apod() -> Result<curious::CuriousLine, String> {
    curious::nasa_apod()
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    curious::open_url(url)
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("index.html".into()))
        .title("Mote — настройки")
        .inner_size(400.0, 540.0)
        .min_inner_size(340.0, 420.0)
        .resizable(true)
        .maximizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .center()
        .visible(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            show_main_window,
            hide_main_window,
            set_always_on_top,
            set_presence_paused,
            set_presence_enabled,
            mark_user_placed,
            shoo_away,
            enter_rage_mode,
            exit_rage_mode,
            media_control,
            media_volume_get,
            media_volume_set,
            digit_copy,
            digit_replace,
            set_hit_regions,
            spell_preload,
            spell_ready,
            spell_offer,
            spell_replace,
            spell_check_selection,
            layout_fix,
            layout_auto_get,
            layout_auto_set,
            spell_clear_target,
            spell_ignore_add,
            llm_status,
            llm_set_device,
            llm_download_model,
            llm_improve_text,
            llm_improve_selection,
            weather_now,
            curious_number_fact,
            curious_date_fact,
            curious_quote,
            curious_wiki,
            curious_wiki_random,
            curious_country,
            curious_book,
            curious_nasa_apod,
            open_external_url,
            open_settings
        ])
        .setup(|app| {
            let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);

            let show_i = MenuItem::with_id(app, "show", "Показать", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "Скрыть", true, None::<&str>)?;
            let reply_i = MenuItem::with_id(app, "reply", "Написать…", true, None::<&str>)?;
            let settings_i = MenuItem::with_id(app, "settings", "Настройки…", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let always_i = CheckMenuItem::with_id(
                app,
                "always_on_top",
                "Поверх окон",
                true,
                true,
                None::<&str>,
            )?;
            let follow_i = CheckMenuItem::with_id(
                app,
                "follow",
                "Следить за курсором",
                true,
                true,
                None::<&str>,
            )?;
            let autostart_i = CheckMenuItem::with_id(
                app,
                "autostart",
                "Автозапуск",
                true,
                autostart_enabled,
                None::<&str>,
            )?;
            let check_update_i = MenuItem::with_id(
                app,
                "check_update",
                "Проверить обновления…",
                true,
                None::<&str>,
            )?;
            let install_update_i = MenuItem::with_id(
                app,
                "install_update",
                "Установить обновление",
                false,
                None::<&str>,
            )?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &show_i,
                    &hide_i,
                    &reply_i,
                    &settings_i,
                    &sep1,
                    &always_i,
                    &follow_i,
                    &autostart_i,
                    &check_update_i,
                    &install_update_i,
                    &sep2,
                    &quit_i,
                ],
            )?;

            let tray_icon =
                Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("tray icon");

            let tray = TrayIconBuilder::with_id("mote-tray")
                .icon(tray_icon)
                .menu(&menu)
                .tooltip("Mote")
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "hide" => {
                        if let Some(window) = main_window(app) {
                            let _ = window.hide();
                        }
                    }
                    "reply" => {
                        show_window(app);
                        let _ = app.emit("mote://focus-reply", ());
                    }
                    "settings" => {
                        let _ = open_settings(app.clone());
                    }
                    "always_on_top" => {
                        if let Some(window) = main_window(app) {
                            let currently = window.is_always_on_top().unwrap_or(true);
                            let _ = window.set_always_on_top(!currently);
                            let _ = always_i.set_checked(!currently);
                        }
                    }
                    "follow" => {
                        let next = !watcher::is_enabled();
                        watcher::set_enabled(next);
                        let _ = follow_i.set_checked(next);
                    }
                    "autostart" => {
                        let manager = app.autolaunch();
                        let enabled = manager.is_enabled().unwrap_or(false);
                        if enabled {
                            let _ = manager.disable();
                            let _ = autostart_i.set_checked(false);
                        } else {
                            let _ = manager.enable();
                            let _ = autostart_i.set_checked(true);
                        }
                    }
                    "check_update" => {
                        update::spawn_check(app.clone());
                    }
                    "install_update" => {
                        update::spawn_install(app.clone());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                })
                .build(app)?;

            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyM);
            app.global_shortcut()
                .on_shortcut(shortcut, |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_window(app);
                    }
                })?;

            let spell_sel = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);
            app.global_shortcut()
                .on_shortcut(spell_sel, |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let handle = app.clone();
                        let _ = std::thread::spawn(move || {
                            if let Err(e) = selection_spell::check_selection(handle.clone()) {
                                eprintln!("[mote] selection spell: {e}");
                                let _ = handle.emit("mote://selection-spell-status", e);
                            }
                        });
                    }
                })?;

            if let Some(window) = main_window(app.handle()) {
                let _ = window.set_always_on_top(true);
            }

            watcher::start(app.handle().clone());
            media_session::start(app.handle().clone());
            spellcheck::preload(app.handle().clone());
            spell_ignore::init();
            layout_fix::init_auto_from_prefs();
            canvas_typing::start(app.handle().clone());
            selection_spell::start(app.handle().clone());
            llm::warmup_on_startup(app.handle().clone());
            update::init_state(app.handle());
            update::schedule_startup_check(app.handle().clone());

            let install_on = install_update_i.clone();
            let handle = app.handle().clone();
            let _ = handle.listen("mote://update-available", move |_| {
                let _ = install_on.set_enabled(true);
            });
            let install_off = install_update_i.clone();
            let _ = handle.listen("mote://update-none", move |_| {
                let _ = install_off.set_enabled(false);
            });
            let tray_for_status = tray.clone();
            let _ = handle.listen("mote://update-status", move |event| {
                let raw = event.payload();
                let msg = serde_json::from_str::<String>(raw)
                    .unwrap_or_else(|_| raw.trim_matches('"').to_string());
                let tip = if msg.is_empty() {
                    "Mote".to_string()
                } else {
                    format!("Mote — {msg}")
                };
                let _ = tray_for_status.set_tooltip(Some(&tip));
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running Mote")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                llm::stop_sidecar();
                selection_spell::stop();
                canvas_typing::stop();
            }
        });
}
