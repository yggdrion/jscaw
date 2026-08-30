#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi(object)]
pub struct AudioSession {
    pub pid: u32,
    pub process_name: String,
    pub volume: f64,
    pub muted: bool,
}

fn validate_volume(volume: f64) -> Result<()> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("volume must be a finite number between 0 and 1, got {volume}"),
        ));
    }
    Ok(())
}

#[napi]
pub fn list_sessions() -> Vec<AudioSession> {
    Vec::new()
}

#[napi]
pub fn set_process_volume(_process_name: String, volume: f64) -> Result<u32> {
    validate_volume(volume)?;
    Ok(0)
}

#[napi]
pub fn set_process_mute(_process_name: String, _muted: bool) -> u32 {
    0
}
