use crate::com::{resolve_device, to_napi_err, ComGuard};
use napi::Result;
use std::path::Path;
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionManager2, ISimpleAudioVolume,
};
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

pub struct AudioSessionInfo {
    pub pid: u32,
    pub process_name: String,
    pub volume: f32,
    pub muted: bool,
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

/// Returns `Ok(None)` when there is no default render device, rather than an error.
fn session_manager() -> Result<Option<IAudioSessionManager2>> {
    let Some(device) = resolve_device(None)? else {
        return Ok(None);
    };
    unsafe { device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }
        .map(Some)
        .map_err(|e| to_napi_err("failed to activate audio session manager", e))
}

/// Walks every active audio session on the default render endpoint, calling `visit` with
/// each session's process id and volume control. Sessions with no resolvable process (dead
/// process, access denied, system sounds) are skipped rather than failing the whole call.
/// A machine with no default render device (e.g. headless CI) yields an empty result.
fn each_session<T>(mut visit: impl FnMut(u32, &ISimpleAudioVolume) -> Option<T>) -> Result<Vec<T>> {
    let _com = ComGuard::new().map_err(|e| to_napi_err("failed to initialize COM", e))?;
    let Some(manager) = session_manager()? else {
        return Ok(Vec::new());
    };
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
