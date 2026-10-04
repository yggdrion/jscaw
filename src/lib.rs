#![deny(clippy::all)]

mod com;
mod devices;
mod sessions;

use devices::{Device, DeviceFlow, DeviceRole, ListDevicesOptions};
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
    Ok(sessions::list_sessions()?
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
pub fn set_process_volume(process_name: String, volume: f64) -> napi::Result<u32> {
    com::validate_volume(volume)?;
    sessions::set_volume_for_process(&process_name, volume as f32)
}

#[napi]
pub fn set_process_mute(process_name: String, muted: bool) -> napi::Result<u32> {
    sessions::set_mute_for_process(&process_name, muted)
}

#[napi]
pub fn list_devices(options: Option<ListDevicesOptions>) -> napi::Result<Vec<Device>> {
    devices::list_devices(options)
}

#[napi]
pub fn get_default_device(
    flow: Option<DeviceFlow>,
    role: Option<DeviceRole>,
) -> napi::Result<Option<Device>> {
    devices::get_default_device(flow, role)
}

#[napi]
pub fn get_device(id: String) -> napi::Result<Option<Device>> {
    devices::get_device(id)
}
