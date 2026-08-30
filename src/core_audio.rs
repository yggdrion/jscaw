use napi::{Error, Result, Status};
use std::path::Path;
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionManager2,
    IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

pub struct AudioSessionInfo {
    pub pid: u32,
    pub process_name: String,
    pub volume: f32,
    pub muted: bool,
}

pub fn validate_volume(volume: f64) -> Result<()> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("volume must be a finite number between 0 and 1, got {volume}"),
        ));
    }
    Ok(())
}

fn to_napi_err(context: &str, err: windows::core::Error) -> Error {
    Error::new(
        Status::GenericFailure,
        format!(
            "{context}: {} (0x{:08X})",
            err.message(),
            err.code().0 as u32
        ),
    )
}

/// RAII guard: initializes COM for this thread on construction, uninitializes on drop.
/// napi-rs runs each synchronous call on a fresh worker thread, so this is cheap.
struct ComGuard;

impl ComGuard {
    fn new() -> windows::core::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn process_name_for_pid(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; MAX_PATH as usize];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

fn session_manager() -> Result<IAudioSessionManager2> {
    unsafe {
        let device_enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| to_napi_err("failed to create audio device enumerator", e))?;
        let device = device_enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| to_napi_err("failed to get default audio endpoint", e))?;
        device
            .Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
            .map_err(|e| to_napi_err("failed to activate audio session manager", e))
    }
}

/// Walks every active audio session on the default render endpoint, calling `visit` with
/// each session's process id and volume control. Sessions with no resolvable process (dead
/// process, access denied, system sounds) are skipped rather than failing the whole call.
fn each_session<T>(mut visit: impl FnMut(u32, &ISimpleAudioVolume) -> Option<T>) -> Result<Vec<T>> {
    let _com = ComGuard::new().map_err(|e| to_napi_err("failed to initialize COM", e))?;
    let manager = session_manager()?;
    let mut results = Vec::new();
    unsafe {
        let session_enumerator: IAudioSessionEnumerator = manager
            .GetSessionEnumerator()
            .map_err(|e| to_napi_err("failed to enumerate audio sessions", e))?;
        let count = session_enumerator
            .GetCount()
            .map_err(|e| to_napi_err("failed to get audio session count", e))?;
        for i in 0..count {
            let Ok(control) = session_enumerator.GetSession(i) else {
                continue;
            };
            let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let Ok(pid) = control2.GetProcessId() else {
                continue;
            };
            if pid == 0 {
                continue;
            }
            let Ok(simple_volume) = control2.cast::<ISimpleAudioVolume>() else {
                continue;
            };
            if let Some(value) = visit(pid, &simple_volume) {
                results.push(value);
            }
        }
    }
    Ok(results)
}

pub fn list_sessions() -> Result<Vec<AudioSessionInfo>> {
    each_session(|pid, simple_volume| {
        let process_name = process_name_for_pid(pid)?;
        unsafe {
            let volume = simple_volume.GetMasterVolume().ok()?;
            let muted = simple_volume.GetMute().ok()?.as_bool();
            Some(AudioSessionInfo {
                pid,
                process_name,
                volume,
                muted,
            })
        }
    })
}

fn matching_sessions(
    process_name: &str,
    mut apply: impl FnMut(&ISimpleAudioVolume) -> bool,
) -> Result<u32> {
    let updated = each_session(|pid, simple_volume| {
        let name = process_name_for_pid(pid)?;
        if !name.eq_ignore_ascii_case(process_name) {
            return None;
        }
        apply(simple_volume).then_some(())
    })?;
    Ok(updated.len() as u32)
}

pub fn set_volume_for_process(process_name: &str, volume: f32) -> Result<u32> {
    matching_sessions(process_name, |simple_volume| {
        unsafe { simple_volume.SetMasterVolume(volume, std::ptr::null()) }.is_ok()
    })
}

pub fn set_mute_for_process(process_name: &str, muted: bool) -> Result<u32> {
    matching_sessions(process_name, |simple_volume| {
        unsafe { simple_volume.SetMute(muted, std::ptr::null()) }.is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::validate_volume;

    #[test]
    fn accepts_mid_range_volume() {
        assert!(validate_volume(0.5).is_ok());
    }

    #[test]
    fn rejects_below_zero() {
        assert!(validate_volume(-0.01).is_err());
    }

    #[test]
    fn rejects_above_one() {
        assert!(validate_volume(1.01).is_err());
    }
}
