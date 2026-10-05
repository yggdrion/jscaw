#![deny(clippy::all)]

mod com;
mod devices;
mod endpoint;
mod events;
mod policy_config;
mod sessions;

use devices::{Device, DeviceFlow, DeviceRole, ListDevicesOptions, PropertyValue};
use endpoint::{EndpointVolume, StepDirection};
use napi::bindgen_prelude::{Function, Unknown};
use napi::Env;
use napi_derive::napi;
use sessions::{AudioSession, ListSessionsOptions, SessionTarget};
use std::collections::HashMap;

#[napi]
pub fn list_sessions(options: Option<ListSessionsOptions>) -> napi::Result<Vec<AudioSession>> {
    sessions::list_sessions(options)
}

#[napi]
pub fn set_process_volume(process_name: String, volume: f64) -> napi::Result<u32> {
    sessions::set_volume(SessionTarget::process(process_name), volume)
}

#[napi]
pub fn set_process_mute(process_name: String, muted: bool) -> napi::Result<u32> {
    sessions::set_mute(SessionTarget::process(process_name), muted)
}

#[napi]
pub fn set_session_volume(target: SessionTarget, volume: f64) -> napi::Result<u32> {
    sessions::set_volume(target, volume)
}

#[napi]
pub fn set_session_mute(target: SessionTarget, muted: bool) -> napi::Result<u32> {
    sessions::set_mute(target, muted)
}

#[napi]
pub fn set_session_display_name(target: SessionTarget, display_name: String) -> napi::Result<u32> {
    sessions::set_display_name(target, display_name)
}

#[napi]
pub fn set_session_icon_path(target: SessionTarget, icon_path: String) -> napi::Result<u32> {
    sessions::set_icon_path(target, icon_path)
}

#[napi]
pub fn set_session_grouping_param(
    target: SessionTarget,
    grouping_param: String,
) -> napi::Result<u32> {
    sessions::set_grouping_param(target, grouping_param)
}

#[napi]
pub fn set_session_ducking_preference(target: SessionTarget, opt_out: bool) -> napi::Result<u32> {
    sessions::set_ducking_preference(target, opt_out)
}

#[napi]
pub fn get_session_channel_volumes(target: SessionTarget) -> napi::Result<Vec<Vec<f64>>> {
    sessions::get_channel_volumes(target)
}

#[napi]
pub fn set_session_channel_volume(
    target: SessionTarget,
    channel: f64,
    volume: f64,
) -> napi::Result<u32> {
    sessions::set_channel_volume(target, channel, volume)
}

#[napi]
pub fn get_session_peak(target: SessionTarget) -> napi::Result<Vec<f64>> {
    sessions::get_peak(target)
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

#[napi(ts_return_type = "Record<string, string | number | boolean | null> | null")]
pub fn get_device_properties(
    device_id: String,
) -> napi::Result<Option<HashMap<String, Option<PropertyValue>>>> {
    devices::get_device_properties(device_id)
}

#[napi]
pub fn get_endpoint_volume(device_id: Option<String>) -> napi::Result<Option<EndpointVolume>> {
    endpoint::get_endpoint_volume(device_id.as_deref())
}

#[napi]
pub fn get_endpoint_peak(device_id: Option<String>) -> napi::Result<Option<f64>> {
    endpoint::get_peak(device_id.as_deref())
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
    channel: f64,
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

#[napi]
pub fn set_default_device(device_id: String, roles: Option<Vec<DeviceRole>>) -> napi::Result<bool> {
    policy_config::set_default_device(device_id, roles)
}

/// Subscribes to device add/remove/state/default/property changes. The subscription keeps
/// the process alive until the returned `unsubscribe` is called.
#[napi(
    strict,
    ts_args_type = "callback: (event: DeviceEvent) => void",
    ts_return_type = "() => void"
)]
pub fn on_device_event<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
) -> napi::Result<Function<'e, (), ()>> {
    events::device::on_device_event(env, callback)
}

/// Subscribes to master volume, mute and channel volume changes on `deviceId` (default render
/// device when omitted). Returns `null` when the device doesn't exist; otherwise the
/// subscription keeps the process alive until the returned `unsubscribe` is called.
#[napi(
    strict,
    ts_args_type = "callback: (event: EndpointVolumeEvent) => void, deviceId?: string",
    ts_return_type = "(() => void) | null"
)]
pub fn on_endpoint_volume_change<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
    device_id: Option<String>,
) -> napi::Result<Option<Function<'e, (), ()>>> {
    events::endpoint::on_endpoint_volume_change(env, callback, device_id.as_deref())
}

/// Subscribes to new audio sessions on `deviceId` (default render device when omitted); the
/// callback gets the session as `listSessions` reports it. Returns `null` when the device
/// doesn't exist; otherwise the subscription keeps the process alive until `unsubscribe`.
#[napi(
    strict,
    ts_args_type = "callback: (session: AudioSession) => void, deviceId?: string",
    ts_return_type = "(() => void) | null"
)]
pub fn on_session_created<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
    device_id: Option<String>,
) -> napi::Result<Option<Function<'e, (), ()>>> {
    events::session::on_session_created(env, callback, device_id.as_deref())
}

/// Subscribes to volume, mute, metadata, state and disconnect changes on the sessions that
/// match `target` now (sessions created later aren't picked up). Returns `null` when nothing
/// matches; otherwise the subscription keeps the process alive until `unsubscribe`.
#[napi(
    strict,
    ts_args_type = "target: SessionTarget, callback: (event: SessionEvent) => void",
    ts_return_type = "(() => void) | null"
)]
pub fn on_session_event<'e>(
    env: &'e Env,
    target: SessionTarget,
    callback: Function<Unknown<'static>, ()>,
) -> napi::Result<Option<Function<'e, (), ()>>> {
    events::session::on_session_event(env, target, callback)
}

/// Subscribes to Windows ducking other sessions for a communications stream on `deviceId`
/// (default render device when omitted). Returns `null` when the device doesn't exist;
/// otherwise the subscription keeps the process alive until `unsubscribe`.
#[napi(
    strict,
    ts_args_type = "callback: (event: DuckEvent) => void, deviceId?: string",
    ts_return_type = "(() => void) | null"
)]
pub fn on_duck_event<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
    device_id: Option<String>,
) -> napi::Result<Option<Function<'e, (), ()>>> {
    events::session::on_duck_event(env, callback, device_id.as_deref())
}
