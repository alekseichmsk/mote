//! Download GGUF + llama-server runtime into %APPDATA%\Mote with progress events.

use super::{
    device_bin_dir, find_runtime_bin, load_device, model_path, models_dir, runtime_present_for,
    LlmDevice, MODEL_FILE, MODEL_SHA256, MODEL_URL, RUNTIME_CPU_ZIP_URL, RUNTIME_CUDART_ZIP_URL,
    RUNTIME_CUDA_ZIP_URL, RUNTIME_EXE, RUNTIME_SHA256, RUNTIME_TAG,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

static DOWNLOAD_BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgress {
    pub phase: String,
    pub percent: u8,
    pub detail: String,
}

/// Download missing pieces (model and/or runtime for active device) in one worker.
pub fn start_download_missing(app: AppHandle) -> Result<(), String> {
    let device = load_device();
    let need_model = {
        let dest = model_path();
        !(dest.is_file()
            && fs::metadata(&dest)
                .map(|m| m.len() > 1_000_000)
                .unwrap_or(false))
    };
    let need_runtime = !runtime_present_for(device);
    if !need_model && !need_runtime {
        emit(
            &app,
            "ready",
            100,
            format!("уже скачано (модель и движок {})", device.as_str()),
        );
        return Ok(());
    }
    if DOWNLOAD_BUSY.swap(true, Ordering::SeqCst) {
        emit(&app, "download", 0, "уже скачивается…");
        return Ok(());
    }
    emit(&app, "download", 0, "старт…");
    let app2 = app.clone();
    thread::Builder::new()
        .name("mote-llm-download".into())
        .spawn(move || {
            let result = download_missing_blocking(&app2, need_model, need_runtime, device);
            DOWNLOAD_BUSY.store(false, Ordering::SeqCst);
            if let Err(e) = result {
                eprintln!("[mote] llm: download err: {e}");
                emit(&app2, "error", 0, e);
            }
        })
        .map_err(|e| {
            DOWNLOAD_BUSY.store(false, Ordering::SeqCst);
            format!("download spawn: {e}")
        })?;
    Ok(())
}

pub fn start_runtime_download(app: AppHandle) -> Result<(), String> {
    let device = load_device();
    if runtime_present_for(device) {
        emit(
            &app,
            "ready",
            100,
            format!("движок {} уже на месте ({RUNTIME_TAG})", device.as_str()),
        );
        return Ok(());
    }
    if DOWNLOAD_BUSY.swap(true, Ordering::SeqCst) {
        emit(&app, "download", 0, "движок: уже скачивается…");
        return Ok(());
    }
    emit(&app, "download", 0, "движок: старт…");
    let app2 = app.clone();
    thread::Builder::new()
        .name("mote-llm-runtime".into())
        .spawn(move || {
            let result = download_runtime_blocking(&app2, device);
            DOWNLOAD_BUSY.store(false, Ordering::SeqCst);
            match result {
                Ok(()) => {
                    emit(
                        &app2,
                        "ready",
                        100,
                        format!("движок {} готов ({RUNTIME_TAG})", device.as_str()),
                    );
                }
                Err(e) => {
                    eprintln!("[mote] llm: runtime download err: {e}");
                    emit(&app2, "error", 0, e);
                }
            }
        })
        .map_err(|e| {
            DOWNLOAD_BUSY.store(false, Ordering::SeqCst);
            format!("runtime spawn: {e}")
        })?;
    Ok(())
}

fn download_missing_blocking(
    app: &AppHandle,
    need_model: bool,
    need_runtime: bool,
    device: LlmDevice,
) -> Result<(), String> {
    if need_model {
        download_model_blocking(app)?;
    }
    if need_runtime {
        download_runtime_blocking(app, device)?;
    }
    emit(app, "ready", 100, "готово");
    Ok(())
}

fn emit(app: &AppHandle, phase: &str, percent: u8, detail: impl Into<String>) {
    let _ = app.emit(
        "mote://model-progress",
        ModelProgress {
            phase: phase.into(),
            percent,
            detail: detail.into(),
        },
    );
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(60))
        .timeout_read(Duration::from_secs(120))
        .build()
}

fn download_to_file(
    app: &AppHandle,
    url: &str,
    part: &Path,
    label: &str,
    size_unit_gb: bool,
) -> Result<(u64, String), String> {
    emit(app, "download", 0, format!("{label}: подключение…"));
    let resp = http_agent()
        .get(url)
        .call()
        .map_err(|e| format!("http: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut file = File::create(part).map_err(|e| format!("create part: {e}"))?;
    let mut buf = [0u8; 64 * 1024];
    let mut written: u64 = 0;
    let mut hasher = Sha256::new();
    let mut last_pct: u8 = 255;

    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("write: {e}"))?;
        hasher.update(&buf[..n]);
        written += n as u64;
        let pct = written
            .checked_mul(100)
            .and_then(|v| v.checked_div(total))
            .map(|v| v.min(99) as u8)
            .unwrap_or(0);
        if pct != last_pct {
            last_pct = pct;
            let detail = if size_unit_gb {
                format!(
                    "{label}: {:.1} / {:.1} ГБ",
                    written as f64 / 1e9,
                    total as f64 / 1e9
                )
            } else {
                format!(
                    "{label}: {:.1} / {:.1} МБ",
                    written as f64 / 1e6,
                    total as f64 / 1e6
                )
            };
            emit(app, "download", pct, detail);
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    Ok((written, hex::encode(hasher.finalize())))
}

fn download_model_blocking(app: &AppHandle) -> Result<(), String> {
    let dir = models_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("models dir: {e}"))?;
    let dest = model_path();
    let part = dir.join(format!("{MODEL_FILE}.part"));
    eprintln!("[mote] llm: download model → {}", dest.display());

    let (written, digest) = download_to_file(app, MODEL_URL, &part, "модель", true)?;
    if !MODEL_SHA256.is_empty() && digest != MODEL_SHA256 {
        let _ = fs::remove_file(&part);
        return Err(format!("SHA256 модели не совпал ({digest})"));
    }
    if MODEL_SHA256.is_empty() {
        eprintln!("[mote] llm: model sha256={digest} (verify skipped)");
    }

    if dest.exists() {
        let _ = fs::remove_file(&dest);
    }
    fs::rename(&part, &dest).map_err(|e| format!("rename: {e}"))?;
    eprintln!(
        "[mote] llm: model ok bytes={} path={}",
        written,
        dest.display()
    );
    emit(
        app,
        "download",
        100,
        format!("модель готова ({:.1} ГБ)", written as f64 / 1e9),
    );
    Ok(())
}

fn download_runtime_blocking(app: &AppHandle, device: LlmDevice) -> Result<(), String> {
    match device {
        LlmDevice::Cpu => download_cpu_runtime(app),
        LlmDevice::Gpu => download_cuda_runtime(app),
    }
}

fn download_cpu_runtime(app: &AppHandle) -> Result<(), String> {
    let dir = device_bin_dir(LlmDevice::Cpu);
    fs::create_dir_all(&dir).map_err(|e| format!("bin/cpu dir: {e}"))?;
    let written = download_and_extract_zip(
        app,
        RUNTIME_CPU_ZIP_URL,
        &dir,
        "движок CPU",
        &format!("llama-cpu-{RUNTIME_TAG}"),
    )?;
    finalize_runtime(app, LlmDevice::Cpu, written)
}

fn download_cuda_runtime(app: &AppHandle) -> Result<(), String> {
    let dir = device_bin_dir(LlmDevice::Gpu);
    fs::create_dir_all(&dir).map_err(|e| format!("bin/cuda dir: {e}"))?;
    let written_cuda = download_and_extract_zip(
        app,
        RUNTIME_CUDA_ZIP_URL,
        &dir,
        "движок CUDA",
        &format!("llama-cuda-{RUNTIME_TAG}"),
    )?;
    let written_cudart = download_and_extract_zip(
        app,
        RUNTIME_CUDART_ZIP_URL,
        &dir,
        "CUDA runtime",
        &format!("cudart-{RUNTIME_TAG}"),
    )?;
    finalize_runtime(
        app,
        LlmDevice::Gpu,
        written_cuda.saturating_add(written_cudart),
    )
}

fn download_and_extract_zip(
    app: &AppHandle,
    url: &str,
    dir: &Path,
    label: &str,
    stem: &str,
) -> Result<u64, String> {
    let zip_part = dir.join(format!("{stem}.zip.part"));
    let zip_path = dir.join(format!("{stem}.zip"));
    eprintln!(
        "[mote] llm: download {label} {RUNTIME_TAG} → {}",
        dir.display()
    );

    let (written, digest) = download_to_file(app, url, &zip_part, label, false)?;
    if !RUNTIME_SHA256.is_empty() && digest != RUNTIME_SHA256 {
        let _ = fs::remove_file(&zip_part);
        return Err(format!("SHA256 {label} не совпал ({digest})"));
    }
    if RUNTIME_SHA256.is_empty() {
        eprintln!("[mote] llm: {label} sha256={digest} (verify skipped)");
    }

    if zip_path.exists() {
        let _ = fs::remove_file(&zip_path);
    }
    fs::rename(&zip_part, &zip_path).map_err(|e| format!("rename zip: {e}"))?;

    emit(app, "download", 99, format!("{label}: распаковка…"));
    extract_zip(&zip_path, dir)?;
    let _ = fs::remove_file(&zip_path);
    flatten_runtime_files(dir);
    Ok(written)
}

fn finalize_runtime(app: &AppHandle, device: LlmDevice, written: u64) -> Result<(), String> {
    let dir = device_bin_dir(device);
    let found = find_runtime_bin(device).ok_or_else(|| {
        format!(
            "{RUNTIME_EXE} не найден после распаковки в {}",
            dir.display()
        )
    })?;
    let exe = promote_exe_to_bin_root(&found, &dir)?;
    let version_path = dir.join("runtime.version");
    let _ = fs::write(
        &version_path,
        format!("{RUNTIME_TAG}-{}", device.bin_subdir()),
    );

    if !runtime_present_for(device) {
        return Err(format!(
            "движок {} после распаковки не прошёл проверку",
            device.as_str()
        ));
    }

    eprintln!(
        "[mote] llm: runtime {} ok bytes={} exe={}",
        device.as_str(),
        written,
        exe.display()
    );
    emit(
        app,
        "download",
        100,
        format!(
            "движок {} готов ({RUNTIME_TAG}, {:.1} МБ)",
            device.as_str(),
            written as f64 / 1e6
        ),
    );
    Ok(())
}

fn extract_zip(zip: &Path, dest: &Path) -> Result<(), String> {
    // Windows 10+ tar.exe extracts zip; keeps DLLs next to llama-server.
    let status = Command::new("tar")
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(dest)
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    if !status.success() {
        return Err("не удалось распаковать движок (tar)".into());
    }
    Ok(())
}

fn promote_exe_to_bin_root(exe: &Path, bin: &Path) -> Result<PathBuf, String> {
    flatten_runtime_files(bin);
    let target = bin.join(RUNTIME_EXE);
    if target.is_file() {
        Ok(target)
    } else if exe.is_file() {
        Ok(exe.to_path_buf())
    } else {
        Err(format!("{RUNTIME_EXE} не найден в {}", bin.display()))
    }
}

/// Copy nested .exe/.dll up to the device bin root (cuda zip + cudart layouts differ).
fn flatten_runtime_files(bin: &Path) {
    let mut stack = vec![bin.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let src = entry.path();
            if src.is_dir() {
                if src != bin {
                    stack.push(src);
                }
                continue;
            }
            if !src.is_file() {
                continue;
            }
            let Some(name) = src.file_name() else {
                continue;
            };
            let name_str = name.to_string_lossy().to_ascii_lowercase();
            if !(name_str.ends_with(".dll") || name_str.ends_with(".exe")) {
                continue;
            }
            let dst = bin.join(name);
            if src == dst {
                continue;
            }
            if let Err(e) = fs::copy(&src, &dst) {
                eprintln!(
                    "[mote] llm: flatten copy err {} → {}: {e}",
                    src.display(),
                    dst.display()
                );
            }
        }
    }
}
