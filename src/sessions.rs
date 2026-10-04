use crate::com::{
    activate, is_missing_device, resolve_device, take_co_string, to_napi_err, ComGuard,
};
use napi::Result;
use napi_derive::napi;
use std::path::Path;
use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Foundation::{CloseHandle, MAX_PATH, S_OK};
use windows::Win32::Media::Audio::{
    AudioSessionState, AudioSessionStateActive, AudioSessionStateExpired,
    AudioSessionStateInactive, IAudioSessionControl2, IAudioSessionEnumerator,
    IAudioSessionManager2, ISimpleAudioVolume,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[napi(string_enum)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    #[napi(value = "inactive")]
    Inactive,
    #[napi(value = "active")]
    Active,
    #[napi(value = "expired")]
    Expired,
}

impl SessionState {
    fn from_windows(state: AudioSessionState) -> Option<Self> {
        match state {
            s if s == AudioSessionStateInactive => Some(Self::Inactive),
            s if s == AudioSessionStateActive => Some(Self::Active),
            s if s == AudioSessionStateExpired => Some(Self::Expired),
            _ => None,
        }
    }
}

#[napi(object)]
pub struct AudioSession {
    pub pid: u32,
    /// Executable file name; `''` for the system sounds session.
    pub process_name: String,
    pub volume: f64,
    pub muted: bool,
    pub state: SessionState,
    /// May be empty or an indirect string such as `@%SystemRoot%\...`, exactly as Windows stores it.
    pub display_name: String,
    pub icon_path: String,
    /// `{XXXXXXXX-…}`; sessions that share one are grouped in the volume mixer.
    pub grouping_param: String,
    pub session_id: String,
    pub instance_id: String,
    pub is_system_sounds: bool,
}

#[napi(object)]
pub struct ListSessionsOptions {
    pub device_id: Option<String>,
    pub include_system_sounds: Option<bool>,
}

fn format_guid(guid: GUID) -> String {
    format!("{{{guid:?}}}")
}

/// Reads a COM-allocated string getter's result; empty when the getter failed.
fn read_string(result: windows::core::Result<PWSTR>) -> String {
    result
        .map(|raw| unsafe { take_co_string(raw) })
        .unwrap_or_default()
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

/// `Ok(None)` when the device is missing, disabled or unplugged.
fn session_manager(device_id: Option<&str>) -> Result<Option<IAudioSessionManager2>> {
    let Some(device) = resolve_device(device_id)? else {
        return Ok(None);
    };
    activate(&device, "failed to activate audio session manager")
}

/// Walks every audio session on `device_id` (default render endpoint when `None`), calling
/// `visit` with each session's control and process id (0 for system sounds). Sessions whose
/// control or pid can't be read are skipped rather than failing the whole call. A missing,
/// disabled or unplugged device yields an empty result.
fn each_session<T>(
    device_id: Option<&str>,
    mut visit: impl FnMut(&IAudioSessionControl2, u32) -> Option<T>,
) -> Result<Vec<T>> {
    let _com = ComGuard::new().map_err(|e| to_napi_err("failed to initialize COM", e))?;
    let Some(manager) = session_manager(device_id)? else {
        return Ok(Vec::new());
    };
    let mut results = Vec::new();
    unsafe {
        // A not-present endpoint can activate fine and only fail here.
        let session_enumerator: IAudioSessionEnumerator = match manager.GetSessionEnumerator() {
            Ok(enumerator) => enumerator,
            Err(e) if is_missing_device(&e) => return Ok(Vec::new()),
            Err(e) => return Err(to_napi_err("failed to enumerate audio sessions", e)),
        };
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
            if let Some(value) = visit(&control2, pid) {
                results.push(value);
            }
        }
    }
    Ok(results)
}

pub fn list_sessions(options: Option<ListSessionsOptions>) -> Result<Vec<AudioSession>> {
    let (device_id, include_system_sounds) = options.map_or((None, false), |o| {
        (o.device_id, o.include_system_sounds.unwrap_or(false))
    });
    each_session(device_id.as_deref(), |control, pid| unsafe {
        // IsSystemSoundsSession returns S_OK for yes and S_FALSE for no.
        let is_system_sounds = control.IsSystemSoundsSession() == S_OK;
        if is_system_sounds && !include_system_sounds {
            return None;
        }
        // Any other session whose process has exited or can't be opened is skipped, as before.
        let process_name = if is_system_sounds {
            String::new()
        } else {
            process_name_for_pid(pid)?
        };
        let simple_volume = control.cast::<ISimpleAudioVolume>().ok()?;
        Some(AudioSession {
            pid,
            process_name,
            volume: simple_volume.GetMasterVolume().ok()? as f64,
            muted: simple_volume.GetMute().ok()?.as_bool(),
            state: SessionState::from_windows(control.GetState().ok()?)?,
            display_name: read_string(control.GetDisplayName()),
            icon_path: read_string(control.GetIconPath()),
            grouping_param: control
                .GetGroupingParam()
                .map(format_guid)
                .unwrap_or_default(),
            session_id: read_string(control.GetSessionIdentifier()),
            instance_id: read_string(control.GetSessionInstanceIdentifier()),
            is_system_sounds,
        })
    })
}

fn matching_sessions(
    process_name: &str,
    mut apply: impl FnMut(&ISimpleAudioVolume) -> bool,
) -> Result<u32> {
    let updated = each_session(None, |control, pid| {
        let name = process_name_for_pid(pid)?;
        if !name.eq_ignore_ascii_case(process_name) {
            return None;
        }
        let simple_volume = control.cast::<ISimpleAudioVolume>().ok()?;
        apply(&simple_volume).then_some(())
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
    use super::*;

    #[test]
    fn session_state_maps_windows_constants() {
        assert_eq!(
            SessionState::from_windows(AudioSessionStateInactive),
            Some(SessionState::Inactive)
        );
        assert_eq!(
            SessionState::from_windows(AudioSessionStateActive),
            Some(SessionState::Active)
        );
        assert_eq!(
            SessionState::from_windows(AudioSessionStateExpired),
            Some(SessionState::Expired)
        );
        assert_eq!(SessionState::from_windows(AudioSessionState(7)), None);
    }

    #[test]
    fn guid_formats_braced_uppercase() {
        let guid = GUID::from_u128(0x6a1d3b2c_0000_4000_8000_00000000c0de);
        assert_eq!(format_guid(guid), "{6A1D3B2C-0000-4000-8000-00000000C0DE}");
    }
}
