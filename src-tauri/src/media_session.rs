//! Windows SMTC (System Media Transport Controls) — detect music playing in the background.

use serde::Serialize;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPayload {
    pub playing: bool,
    pub title: String,
    pub artist: String,
}

/// SMTC transport: `pause` | `play` | `toggle` | `next` | `previous`
/// System volume: `volume_up` | `volume_down`
pub fn control(action: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::control(action)
    }
    #[cfg(not(windows))]
    {
        let _ = action;
        Err("медиадоступен только на Windows".into())
    }
}

/// Master endpoint volume, `0.0…1.0`.
pub fn get_volume() -> Result<f32, String> {
    #[cfg(windows)]
    {
        win::get_volume()
    }
    #[cfg(not(windows))]
    {
        Err("громкость доступна только на Windows".into())
    }
}

/// Set master endpoint volume, `0.0…1.0`.
pub fn set_volume(level: f32) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::set_volume(level)
    }
    #[cfg(not(windows))]
    {
        let _ = level;
        Err("громкость доступна только на Windows".into())
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };
    use windows::Media::MediaPlaybackType;

    fn session_manager() -> Result<GlobalSystemMediaTransportControlsSessionManager, String> {
        let op = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .map_err(|e| e.to_string())?;
        op.get().map_err(|e| e.to_string())
    }

    fn current_session() -> Result<GlobalSystemMediaTransportControlsSession, String> {
        session_manager()?
            .GetCurrentSession()
            .map_err(|_| "нет активной медиасессии".into())
    }

    fn endpoint_volume(
    ) -> Result<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume, String> {
        use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
        use windows::Win32::Media::Audio::{
            eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
        };
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        };

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| e.to_string())?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| e.to_string())?;
            device
                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                .map_err(|e| e.to_string())
        }
    }

    pub fn get_volume() -> Result<f32, String> {
        unsafe {
            let vol = endpoint_volume()?;
            let level = vol
                .GetMasterVolumeLevelScalar()
                .map_err(|e| e.to_string())?;
            Ok(level.clamp(0.0, 1.0))
        }
    }

    pub fn set_volume(level: f32) -> Result<(), String> {
        let level = level.clamp(0.0, 1.0);
        unsafe {
            let vol = endpoint_volume()?;
            vol.SetMasterVolumeLevelScalar(level, std::ptr::null())
                .map_err(|e| e.to_string())?;
            // Unmute when raising volume from the slider.
            if level > 0.001 {
                let _ = vol.SetMute(false, std::ptr::null());
            }
            Ok(())
        }
    }

    fn tap_volume_key(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        };
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: Default::default(),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
        }
    }

    pub fn control(action: &str) -> Result<(), String> {
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_UP};

        match action {
            "volume_up" => {
                tap_volume_key(VK_VOLUME_UP);
                return Ok(());
            }
            "volume_down" => {
                tap_volume_key(VK_VOLUME_DOWN);
                return Ok(());
            }
            _ => {}
        }

        let session = current_session()?;
        let op = match action {
            "pause" => session.TryPauseAsync(),
            "play" => session.TryPlayAsync(),
            "toggle" => session.TryTogglePlayPauseAsync(),
            "next" => session.TrySkipNextAsync(),
            "previous" => session.TrySkipPreviousAsync(),
            _ => return Err(format!("неизвестное действие: {action}")),
        }
        .map_err(|e| e.to_string())?;
        let _ = op.get().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn snapshot() -> MusicPayload {
        let Ok(manager) = session_manager() else {
            return MusicPayload::default();
        };
        let Ok(session) = manager.GetCurrentSession() else {
            return MusicPayload::default();
        };

        let Ok(playback) = session.GetPlaybackInfo() else {
            return MusicPayload::default();
        };
        let Ok(status) = playback.PlaybackStatus() else {
            return MusicPayload::default();
        };
        let playing = status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
        let paused = status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused;
        // Keep paused sessions so UI can resume; drop everything else.
        if !playing && !paused {
            return MusicPayload::default();
        }

        // Skip video sessions (YouTube etc.) — video presence uses window titles.
        if let Ok(playback_type) = playback.PlaybackType() {
            if let Ok(value) = playback_type.Value() {
                if value == MediaPlaybackType::Video {
                    return MusicPayload::default();
                }
            }
        }

        let mut title = String::new();
        let mut artist = String::new();
        if let Ok(props_op) = session.TryGetMediaPropertiesAsync() {
            if let Ok(props) = props_op.get() {
                if let Ok(t) = props.Title() {
                    title = t.to_string();
                }
                if let Ok(a) = props.Artist() {
                    artist = a.to_string();
                }
            }
        }

        MusicPayload {
            playing,
            title,
            artist,
        }
    }

    pub fn run_loop(app: AppHandle) {
        let mut last = MusicPayload {
            playing: false,
            title: String::from("__unset__"),
            artist: String::new(),
        };
        let mut ticks: u32 = 0;

        loop {
            thread::sleep(Duration::from_millis(1000));
            let next = snapshot();
            ticks = ticks.wrapping_add(1);
            // Emit on change, and heartbeat every 3s while playing so UI stays in sync.
            let heartbeat = next.playing && ticks % 3 == 0;
            if next != last || heartbeat {
                last = next.clone();
                let _ = app.emit("mote://music", next);
            }
        }
    }
}

#[cfg(windows)]
pub fn start(app: AppHandle) {
    thread::spawn(move || win::run_loop(app));
}

#[cfg(not(windows))]
pub fn start(_app: AppHandle) {}
