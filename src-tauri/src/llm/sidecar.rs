//! Spawn / stop llama-server on 127.0.0.1:<free port>.

use super::{find_runtime_bin, LlmDevice};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::AppHandle;

struct SidecarState {
    child: Child,
    port: u16,
    last_used: Instant,
    device: LlmDevice,
}

static STATE: Mutex<Option<SidecarState>> = Mutex::new(None);
static STARTING: Mutex<()> = Mutex::new(());

pub fn current_port() -> u16 {
    STATE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.port))
        .unwrap_or(0)
}

pub fn is_running() -> bool {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let Some(st) = guard.as_mut() else {
        return false;
    };
    match st.child.try_wait() {
        Ok(None) => true,
        Ok(Some(_)) | Err(_) => {
            *guard = None;
            false
        }
    }
}

fn running_device() -> Option<LlmDevice> {
    let guard = match STATE.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    guard.as_ref().map(|s| s.device)
}

pub fn stop_sidecar() {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(mut st) = guard.take() {
        eprintln!("[mote] llm: sidecar stop port={}", st.port);
        let _ = st.child.kill();
        let _ = st.child.wait();
    }
    // Hot-reload / crash can leave extras holding VRAM.
    kill_orphan_mote_sidecars(None);
}

pub fn ensure_running(_app: &AppHandle, model: &Path, device: LlmDevice) -> Result<(), String> {
    let _start_guard = STARTING
        .lock()
        .map_err(|_| "llm sidecar start lock".to_string())?;
    if is_running() {
        if running_device() == Some(device) {
            let keep = STATE
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(|s| s.child.id()));
            kill_orphan_mote_sidecars(keep);
            touch();
            return Ok(());
        }
        eprintln!(
            "[mote] llm: sidecar device change {} → {}",
            running_device().map(|d| d.as_str()).unwrap_or("?"),
            device.as_str()
        );
        stop_sidecar();
    } else {
        kill_orphan_mote_sidecars(None);
    }
    let bin = resolve_llama_bin(device)?;
    let port = free_port()?;
    let ngl = device.ngl();
    eprintln!(
        "[mote] llm: sidecar start port={} device={} ngl={} bin={} model={}",
        port,
        device.as_str(),
        ngl,
        bin.display(),
        model.display()
    );
    let mut cmd = Command::new(&bin);
    // DLLs from the runtime zip live next to the exe.
    if let Some(dir) = bin.parent() {
        cmd.current_dir(dir);
    }
    cmd.arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("-m")
        .arg(model)
        .arg("-c")
        .arg("2048")
        .arg("-ngl")
        .arg(ngl.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("не удалось запустить llama-server: {e}"))?;
    {
        let mut guard = STATE.lock().map_err(|_| "llm sidecar lock".to_string())?;
        *guard = Some(SidecarState {
            child,
            port,
            last_used: Instant::now(),
            device,
        });
    }
    wait_healthy(port, Duration::from_secs(120))?;
    Ok(())
}

fn touch() {
    if let Ok(mut g) = STATE.lock() {
        if let Some(st) = g.as_mut() {
            st.last_used = Instant::now();
        }
    }
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("free port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("free port addr: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// Kill `llama-server.exe` under `%APPDATA%\Mote\bin`, optionally keeping one PID.
#[cfg(windows)]
pub fn kill_orphan_mote_sidecars(keep_pid: Option<u32>) {
    use super::bin_dir;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    let prefix = bin_dir();
    let prefix_lc = prefix.to_string_lossy().to_ascii_lowercase();
    let snap = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(h) => h,
        Err(_) => return,
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) }.is_ok();
    while ok {
        let pid = entry.th32ProcessID;
        let name = {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase()
        };
        if name == "llama-server.exe" && keep_pid != Some(pid) {
            if let Ok(handle) = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                    false,
                    pid,
                )
            } {
                if process_path_under(handle, &prefix_lc) {
                    eprintln!("[mote] llm: orphan sidecar kill pid={pid}");
                    let _ = unsafe { TerminateProcess(handle, 1) };
                }
                let _ = unsafe { CloseHandle(handle) };
            }
        }
        ok = unsafe { Process32NextW(snap, &mut entry) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snap) };

    fn process_path_under(handle: HANDLE, prefix_lc: &str) -> bool {
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle,
                Default::default(),
                PWSTR(buf.as_mut_ptr()),
                &mut size,
            )
        };
        if ok.is_err() || size == 0 {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]).to_ascii_lowercase();
        path.contains(prefix_lc) || path.contains("\\mote\\bin\\")
    }
}

#[cfg(not(windows))]
pub fn kill_orphan_mote_sidecars(_keep_pid: Option<u32>) {}

fn resolve_llama_bin(device: LlmDevice) -> Result<PathBuf, String> {
    // 1) App-managed runtime for the selected device
    if let Some(p) = find_runtime_bin(device) {
        return Ok(p);
    }
    // 2) Dev override only (any device)
    if let Ok(p) = std::env::var("MOTE_LLAMA_SERVER") {
        let path = PathBuf::from(p.trim());
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("MOTE_LLAMA_SERVER не найден: {}", path.display()));
    }
    Err(match device {
        LlmDevice::Gpu => "движок CUDA не найден — скачай в Настройках Mote".into(),
        LlmDevice::Cpu => "движок CPU не найден — скачай в Настройках Mote".into(),
    })
}

fn wait_healthy(port: u16, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    let url = format!("http://127.0.0.1:{port}/health");
    let url_models = format!("http://127.0.0.1:{port}/v1/models");
    while start.elapsed() < timeout {
        if !is_running() {
            return Err("sidecar завершился при старте".into());
        }
        if ureq::get(&url)
            .timeout(Duration::from_secs(2))
            .call()
            .is_ok()
        {
            eprintln!("[mote] llm: sidecar healthy port={port}");
            return Ok(());
        }
        if ureq::get(&url_models)
            .timeout(Duration::from_secs(2))
            .call()
            .is_ok()
        {
            eprintln!("[mote] llm: sidecar models ok port={port}");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    stop_sidecar();
    Err("sidecar не ответил вовремя (модель грузится долго?)".into())
}

/// Idle reaper — call occasionally from watcher or leave for Exit-only stop.
#[allow(dead_code)]
pub fn stop_if_idle(max_idle: Duration) {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(st) = guard.as_ref() {
        if st.last_used.elapsed() >= max_idle {
            if let Some(mut st) = guard.take() {
                eprintln!("[mote] llm: sidecar idle stop");
                let _ = st.child.kill();
                let _ = st.child.wait();
            }
        }
    }
}
