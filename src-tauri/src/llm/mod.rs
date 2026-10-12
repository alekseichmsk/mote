//! Local LLM via llama-server sidecar + GGUF in %APPDATA%\Mote\models.

mod client;
mod download;
mod sidecar;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

pub use sidecar::stop_sidecar;

/// Default on-disk model (Qwen2.5-3B Instruct Q4_K_M).
pub const MODEL_FILE: &str = "Qwen2.5-3B-Instruct-Q4_K_M.gguf";
/// Hugging Face resolve URL (bartowski quant).
pub const MODEL_URL: &str =
    "https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf";
/// Expected SHA-256 (empty = skip verify; fill after trusted download).
pub const MODEL_SHA256: &str = "";

/// llama.cpp release tag for Windows runtimes.
pub const RUNTIME_TAG: &str = "b9992";
pub const RUNTIME_CPU_ZIP_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b9992/llama-b9992-bin-win-cpu-x64.zip";
pub const RUNTIME_CUDA_ZIP_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b9992/llama-b9992-bin-win-cuda-12.4-x64.zip";
pub const RUNTIME_CUDART_ZIP_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b9992/cudart-llama-bin-win-cuda-12.4-x64.zip";
/// Expected SHA-256 of zips (empty = skip).
pub const RUNTIME_SHA256: &str = "";
pub const RUNTIME_EXE: &str = "llama-server.exe";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LlmDevice {
    #[default]
    Gpu,
    Cpu,
}

impl LlmDevice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gpu => "gpu",
            Self::Cpu => "cpu",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "cpu" => Self::Cpu,
            _ => Self::Gpu,
        }
    }

    pub fn ngl(self) -> i32 {
        match self {
            Self::Gpu => 99,
            Self::Cpu => 0,
        }
    }

    pub fn bin_subdir(self) -> &'static str {
        match self {
            Self::Gpu => "cuda",
            Self::Cpu => "cpu",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MotePrefs {
    #[serde(default)]
    llm_device: LlmDevice,
    /// Live EN→RU layout fix after space/Enter (mode A).
    #[serde(default)]
    layout_auto: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmStatus {
    pub ready: bool,
    pub model_path: String,
    pub model_bytes: u64,
    pub model_present: bool,
    pub runtime_present: bool,
    pub runtime_path: String,
    pub sidecar_running: bool,
    pub port: u16,
    pub detail: String,
    pub device: LlmDevice,
    pub gpu_runtime_present: bool,
    pub cpu_runtime_present: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImproveResult {
    pub improved: String,
}

pub fn mote_data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Mote")
}

pub fn models_dir() -> PathBuf {
    mote_data_dir().join("models")
}

pub fn model_path() -> PathBuf {
    models_dir().join(MODEL_FILE)
}

pub fn bin_dir() -> PathBuf {
    mote_data_dir().join("bin")
}

pub fn prefs_path() -> PathBuf {
    mote_data_dir().join("prefs.json")
}

pub fn device_bin_dir(device: LlmDevice) -> PathBuf {
    bin_dir().join(device.bin_subdir())
}

/// Preferred install path after unpack.
pub fn runtime_path_preferred(device: LlmDevice) -> PathBuf {
    device_bin_dir(device).join(RUNTIME_EXE)
}

fn load_prefs() -> MotePrefs {
    migrate_legacy_bin();
    let path = prefs_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return MotePrefs::default();
    };
    match serde_json::from_str::<MotePrefs>(&raw) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[mote] llm: prefs parse err: {e}");
            MotePrefs::default()
        }
    }
}

fn save_prefs(prefs: &MotePrefs) -> Result<(), String> {
    let dir = mote_data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("prefs dir: {e}"))?;
    let raw = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    fs::write(prefs_path(), raw).map_err(|e| format!("prefs write: {e}"))
}

pub fn load_device() -> LlmDevice {
    load_prefs().llm_device
}

pub fn load_layout_auto() -> bool {
    load_prefs().layout_auto
}

pub fn set_device(device: LlmDevice) -> Result<(), String> {
    migrate_legacy_bin();
    let mut prefs = load_prefs();
    prefs.llm_device = device;
    save_prefs(&prefs)?;
    eprintln!("[mote] llm: device set {}", device.as_str());
    sidecar::stop_sidecar();
    Ok(())
}

pub fn set_layout_auto(enabled: bool) -> Result<(), String> {
    let mut prefs = load_prefs();
    prefs.layout_auto = enabled;
    save_prefs(&prefs)?;
    eprintln!("[mote] layout_auto {}", if enabled { "on" } else { "off" });
    Ok(())
}

/// Move flat `%APPDATA%\\Mote\\bin\\llama-server.exe` into `bin\\cpu\\`.
pub fn migrate_legacy_bin() {
    let root = bin_dir();
    let legacy = root.join(RUNTIME_EXE);
    let cpu_dir = device_bin_dir(LlmDevice::Cpu);
    let cpu_exe = cpu_dir.join(RUNTIME_EXE);
    if !legacy.is_file() || cpu_exe.is_file() {
        return;
    }
    if let Err(e) = fs::create_dir_all(&cpu_dir) {
        eprintln!("[mote] llm: migrate bin mkdir err: {e}");
        return;
    }
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let src = entry.path();
        let name = match src.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.eq_ignore_ascii_case("cpu") || name.eq_ignore_ascii_case("cuda") {
            continue;
        }
        if !src.is_file() {
            continue;
        }
        let dst = cpu_dir.join(name);
        if let Err(e) = fs::rename(&src, &dst) {
            if fs::copy(&src, &dst).is_ok() {
                let _ = fs::remove_file(&src);
            } else {
                eprintln!("[mote] llm: migrate bin move err {name}: {e}");
            }
        }
    }
    eprintln!("[mote] llm: migrated legacy bin → {}", cpu_dir.display());
}

pub fn runtime_present_for(device: LlmDevice) -> bool {
    migrate_legacy_bin();
    let Some(exe) = find_runtime_bin(device) else {
        return false;
    };
    runtime_exe_usable(&exe, device)
}

pub fn runtime_present() -> bool {
    runtime_present_for(load_device())
}

fn runtime_exe_usable(exe: &Path, device: LlmDevice) -> bool {
    let Ok(meta) = fs::metadata(exe) else {
        return false;
    };
    if !meta.is_file() || meta.len() < 4_096 {
        return false;
    }
    let Some(dir) = exe.parent() else {
        return false;
    };
    // Recent llama.cpp: tiny stub exe + llama-server-impl.dll (~9 MB).
    let impl_dll = dir.join("llama-server-impl.dll");
    if impl_dll.is_file() {
        let impl_ok = fs::metadata(&impl_dll)
            .map(|m| m.len() > 100_000)
            .unwrap_or(false);
        if !impl_ok {
            return false;
        }
        if device == LlmDevice::Gpu {
            return dir_has_cuda_bits(dir);
        }
        return true;
    }
    // Older single-file builds.
    if meta.len() <= 1_000_000 {
        return false;
    }
    if device == LlmDevice::Gpu {
        return dir_has_cuda_bits(dir);
    }
    true
}

fn dir_has_cuda_bits(dir: &Path) -> bool {
    // Need backend (ggml-cuda) AND CUDA runtime DLLs — otherwise Settings
    // would show «готово» after a partial download and never retry cudart.
    let mut has_backend = false;
    let mut has_runtime = false;
    scan_cuda_bits(dir, &mut has_backend, &mut has_runtime);
    has_backend && has_runtime
}

fn scan_cuda_bits(dir: &Path, has_backend: &mut bool, has_runtime: &mut bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_cuda_bits(&path, has_backend, has_runtime);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.contains("ggml-cuda") {
            *has_backend = true;
        }
        if name.starts_with("cudart") || name.starts_with("cublas") {
            *has_runtime = true;
        }
    }
}

/// Locate llama-server under the device bin folder.
pub fn find_runtime_bin(device: LlmDevice) -> Option<PathBuf> {
    migrate_legacy_bin();
    let preferred = runtime_path_preferred(device);
    if preferred.is_file() {
        return Some(preferred);
    }
    find_file_named(&device_bin_dir(device), RUNTIME_EXE)
}

fn find_file_named(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_file_named(&path, name) {
                return Some(found);
            }
        }
    }
    None
}

pub fn status() -> LlmStatus {
    migrate_legacy_bin();
    let device = load_device();
    let path = model_path();
    let model_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let model_present = path.is_file() && model_bytes > 1_000_000;
    let gpu_runtime_present = runtime_present_for(LlmDevice::Gpu);
    let cpu_runtime_present = runtime_present_for(LlmDevice::Cpu);
    let runtime_present = match device {
        LlmDevice::Gpu => gpu_runtime_present,
        LlmDevice::Cpu => cpu_runtime_present,
    };
    let runtime_bin = find_runtime_bin(device);
    let runtime_path = runtime_bin
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let port = sidecar::current_port();
    let sidecar_running = sidecar::is_running();
    let ready = model_present && runtime_present;
    let detail = if !model_present {
        "модель не скачана".into()
    } else if !runtime_present {
        match device {
            LlmDevice::Gpu => "движок CUDA не скачан".into(),
            LlmDevice::Cpu => "движок CPU не скачан".into(),
        }
    } else if sidecar_running {
        format!("работает {} :{port}", device.as_str())
    } else {
        format!("готов {} · запуск по Улучшить", device.as_str())
    };
    LlmStatus {
        ready,
        model_path: path.display().to_string(),
        model_bytes,
        model_present,
        runtime_present,
        runtime_path,
        sidecar_running,
        port,
        detail,
        device,
        gpu_runtime_present,
        cpu_runtime_present,
    }
}

/// Download model and/or runtime for the active device — whichever is missing.
pub fn download_model(app: AppHandle) -> Result<(), String> {
    download::start_download_missing(app)
}

pub fn download_runtime(app: AppHandle) -> Result<(), String> {
    download::start_runtime_download(app)
}

/// Background GPU warm-up at app start: sidecar + one short improve (discarded).
/// Skips CPU mode and missing model/runtime (no download).
pub fn warmup_on_startup(app: AppHandle) {
    let device = load_device();
    if device != LlmDevice::Gpu {
        eprintln!("[mote] llm: warmup skip device={}", device.as_str());
        return;
    }
    let path = model_path();
    let model_ok = path.is_file()
        && fs::metadata(&path)
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false);
    if !model_ok {
        eprintln!("[mote] llm: warmup skip no model");
        return;
    }
    if !runtime_present_for(LlmDevice::Gpu) {
        eprintln!("[mote] llm: warmup skip no cuda runtime");
        return;
    }
    eprintln!("[mote] llm: warmup start");
    if let Err(e) = std::thread::Builder::new()
        .name("mote-llm-warmup".into())
        .spawn(move || match warmup_blocking(&app) {
            Ok(()) => eprintln!("[mote] llm: warmup ok"),
            Err(err) => eprintln!("[mote] llm: warmup err: {err}"),
        })
    {
        eprintln!("[mote] llm: warmup spawn err: {e}");
    }
}

fn warmup_blocking(app: &AppHandle) -> Result<(), String> {
    ensure_ready(app)?;
    // Tiny request so CUDA kernels compile before the first real «Улучшить».
    let _ = client::improve("ок")?;
    Ok(())
}

/// Ensure model on disk + runtime + sidecar up, then improve text.
pub fn improve_text(app: AppHandle, text: String) -> Result<ImproveResult, String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err("пустой текст".into());
    }
    if trimmed.chars().count() > 4000 {
        return Err("текст слишком длинный для улучшения".into());
    }
    eprintln!(
        "[mote] llm: improve request chars={}",
        trimmed.chars().count()
    );
    ensure_ready(&app)?;
    let raw = client::improve(&trimmed)?;
    if raw.trim().is_empty() {
        return Err("модель вернула пустой ответ".into());
    }
    let improved = prefer_dict_if_model_worse(&app, &trimmed, raw);
    eprintln!("[mote] llm: improve ok chars={}", improved.chars().count());
    Ok(ImproveResult { improved })
}

/// 3B sometimes invents worse typos («пРевеТ» → «Перевет»). Prefer dictionary then.
fn prefer_dict_if_model_worse(app: &AppHandle, source: &str, model_out: String) -> String {
    let out = model_out.trim().to_string();
    if out.is_empty() {
        return out;
    }
    let model_still_bad = crate::spellcheck::spell_offer(app.clone(), out.clone())
        .ok()
        .flatten()
        .is_some();
    let echoed = out == source.trim();
    if !model_still_bad && !echoed {
        return out;
    }
    match crate::spellcheck::spell_offer(app.clone(), source.to_string()) {
        Ok(Some(offer)) => {
            let dict = offer.replaced.trim().to_string();
            if dict.is_empty() || dict == source.trim() {
                return out;
            }
            eprintln!(
                "[mote] llm: improve fallback dict (model_bad={} echo={})",
                model_still_bad, echoed
            );
            dict
        }
        _ => out,
    }
}

fn ensure_ready(app: &AppHandle) -> Result<(), String> {
    eprintln!("[mote] llm: ensure");
    let path = model_path();
    if !path.is_file() {
        return Err("модель не найдена — скачай в Настройках".into());
    }
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() < 1_000_000 {
        return Err("файл модели повреждён или не докачан".into());
    }
    let device = load_device();
    if !runtime_present() {
        return Err(match device {
            LlmDevice::Gpu => "движок CUDA не найден — скачай в Настройках".into(),
            LlmDevice::Cpu => "движок CPU не найден — скачай в Настройках".into(),
        });
    }
    sidecar::ensure_running(app, &path, device)
}
