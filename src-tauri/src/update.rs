//! App auto-update via tauri-plugin-updater (GitHub Releases + signed NSIS).

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Holds the last successful `check()` result until install or a newer check.
pub struct PendingUpdate(pub Mutex<Option<Update>>);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAvailablePayload {
    version: String,
    body: Option<String>,
}

pub fn init_state(app: &AppHandle) {
    app.manage(PendingUpdate(Mutex::new(None)));
}

fn endpoint_ready(app: &AppHandle) -> bool {
    let Some(plugin) = app.config().plugins.0.get("updater") else {
        return false;
    };
    let Some(endpoints) = plugin.get("endpoints").and_then(|v| v.as_array()) else {
        return false;
    };
    endpoints.iter().any(|ep| {
        ep.as_str()
            .map(|s| !s.contains("OWNER/REPO") && s.starts_with("https://"))
            .unwrap_or(false)
    })
}

/// Delayed silent check after startup (does not install).
pub fn schedule_startup_check(app: AppHandle) {
    if cfg!(debug_assertions) {
        eprintln!("[mote] update skip (debug build)");
        return;
    }
    if !endpoint_ready(&app) {
        eprintln!("[mote] update skip (updater endpoint not configured)");
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(8));
        tauri::async_runtime::block_on(async move {
            match check_for_update(app.clone()).await {
                Ok(Some((ver, body))) => {
                    eprintln!("[mote] update available v{ver}");
                    let _ = app.emit(
                        "mote://update-available",
                        UpdateAvailablePayload { version: ver, body },
                    );
                }
                Ok(None) => eprintln!("[mote] update none"),
                Err(e) => eprintln!("[mote] update err: {e}"),
            }
        });
    });
}

pub fn spawn_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        eprintln!("[mote] update check");
        if !endpoint_ready(&app) {
            let msg = "updater endpoint не настроен";
            eprintln!("[mote] update err: {msg}");
            let _ = app.emit("mote://update-status", msg);
            return;
        }
        match check_for_update(app.clone()).await {
            Ok(Some((ver, body))) => {
                eprintln!("[mote] update available v{ver}");
                let _ = app.emit(
                    "mote://update-available",
                    UpdateAvailablePayload {
                        version: ver.clone(),
                        body,
                    },
                );
                let _ = app.emit("mote://update-status", format!("доступно v{ver}"));
            }
            Ok(None) => {
                eprintln!("[mote] update none");
                let _ = app.emit("mote://update-none", ());
                let _ = app.emit("mote://update-status", "обновлений нет");
            }
            Err(e) => {
                eprintln!("[mote] update err: {e}");
                let _ = app.emit("mote://update-none", ());
                let _ = app.emit("mote://update-status", format!("ошибка: {e}"));
            }
        }
    });
}

pub fn spawn_install(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = install_pending(app.clone()).await {
            eprintln!("[mote] update err: {e}");
            let _ = app.emit("mote://update-status", format!("ошибка: {e}"));
        }
    });
}

async fn check_for_update(app: AppHandle) -> Result<Option<(String, Option<String>)>, String> {
    let updater = app.updater().map_err(|e| format!("updater init: {e}"))?;
    let update = updater.check().await.map_err(|e| format!("check: {e}"))?;
    let Some(update) = update else {
        if let Some(state) = app.try_state::<PendingUpdate>() {
            if let Ok(mut g) = state.0.lock() {
                *g = None;
            }
        }
        return Ok(None);
    };
    let version = update.version.clone();
    let body = update.body.clone();
    if let Some(state) = app.try_state::<PendingUpdate>() {
        if let Ok(mut g) = state.0.lock() {
            *g = Some(update);
        }
    }
    Ok(Some((version, body)))
}

async fn install_pending(app: AppHandle) -> Result<(), String> {
    let update = {
        let state = app
            .try_state::<PendingUpdate>()
            .ok_or_else(|| "нет состояния обновления".to_string())?;
        let mut g = state.0.lock().map_err(|_| "pending lock".to_string())?;
        g.take()
    };
    let update = match update {
        Some(u) => u,
        None => {
            eprintln!("[mote] update check (before install)");
            if !endpoint_ready(&app) {
                return Err("updater endpoint не настроен".into());
            }
            let updater = app.updater().map_err(|e| format!("updater init: {e}"))?;
            updater
                .check()
                .await
                .map_err(|e| format!("check: {e}"))?
                .ok_or_else(|| "обновлений нет".to_string())?
        }
    };

    eprintln!("[mote] update downloading v{}", update.version);
    let _ = app.emit(
        "mote://update-status",
        format!("скачиваю v{}…", update.version),
    );

    update
        .download_and_install(
            |_chunk, _total| {},
            || {
                eprintln!("[mote] update download finished");
            },
        )
        .await
        .map_err(|e| format!("install: {e}"))?;

    eprintln!("[mote] update installed");
    let _ = app.emit("mote://update-status", "установлено — перезапуск");
    app.restart();
}
