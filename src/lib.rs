#![deny(clippy::all)]

mod com;
mod devices;
mod endpoint;
mod sessions;

use devices::{Device, DeviceFlow, DeviceRole, ListDevicesOptions};
use endpoint::{EndpointVolume, StepDirection};
use napi_derive::napi;
use sessions::{AudioSession, ListSessionsOptions};

#[napi]
pub fn list_sessions(options: Option<ListSessionsOptions>) -> napi::Result<Vec<AudioSession>> {
    sessions::list_sessions(options)
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

#[napi]
pub fn get_endpoint_volume(device_id: Option<String>) -> napi::Result<Option<EndpointVolume>> {
    endpoint::get_endpoint_volume(device_id.as_deref())
}

#[napi]
pub fn set_endpoint_volume(volume: f64, device_id: Option<String>) -> napi::Result<bool> {
    endpoint::set_volume(volume, device_id.as_deref())
}

#[napi]
pub fn set_endpoint_volume_db(volume_db: f64, device_id: Option<String>) -> napi::Result<bool> {
    endpoint::set_volume_db(volume_db, device_id.as_deref())
}

#[napi]
pub fn set_endpoint_mute(muted: bool, device_id: Option<String>) -> napi::Result<bool> {
    endpoint::set_mute(muted, device_id.as_deref())
}

#[napi]
pub fn set_endpoint_channel_volume(
    channel: u32,
    volume: f64,
    device_id: Option<String>,
) -> napi::Result<bool> {
    endpoint::set_channel_volume(channel, volume, device_id.as_deref())
}

#[napi]
pub fn step_endpoint_volume(
    direction: StepDirection,
    device_id: Option<String>,
) -> napi::Result<bool> {
    endpoint::step(direction, device_id.as_deref())
}
