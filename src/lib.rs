#![deny(clippy::all)]

mod core_audio;

use napi_derive::napi;

#[napi(object)]
pub struct AudioSession {
    pub pid: u32,
    pub process_name: String,
    pub volume: f64,
    pub muted: bool,
}

#[napi]
pub fn list_sessions() -> napi::Result<Vec<AudioSession>> {
    Ok(core_audio::list_sessions()?
        .into_iter()
        .map(|session| AudioSession {
            pid: session.pid,
            process_name: session.process_name,
            volume: session.volume as f64,
            muted: session.muted,
        })
        .collect())
}

#[napi]
pub fn set_process_volume(_process_name: String, volume: f64) -> napi::Result<u32> {
    core_audio::validate_volume(volume)?;
    Ok(0)
}

#[napi]
pub fn set_process_mute(_process_name: String, _muted: bool) -> u32 {
    0
}
