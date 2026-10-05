use crate::com::{
    activate, resolve_device, to_napi_err, validate_index, validate_volume, ComGuard,
};
use napi::{Error, Result, Status};
use napi_derive::napi;
use std::ptr::null;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;

#[napi(object)]
pub struct ChannelVolume {
    pub volume: f64,
    pub volume_db: f64,
}

#[napi(object)]
pub struct VolumeRange {
    pub min_db: f64,
    pub max_db: f64,
    pub increment_db: f64,
}

#[napi(object)]
pub struct VolumeStep {
    pub current: u32,
    pub count: u32,
}

#[napi(object)]
pub struct EndpointVolume {
    pub volume: f64,
    pub volume_db: f64,
    pub muted: bool,
    pub channels: Vec<ChannelVolume>,
    pub range: VolumeRange,
    pub step: VolumeStep,
    /// `ENDPOINT_HARDWARE_SUPPORT_*` bitmask: 1 = volume, 2 = mute, 4 = meter.
    pub hardware_support: u32,
}

/// Runs `f` against the endpoint volume control of `device_id` (default render/console when
/// `None`). Returns `Ok(None)` when the device doesn't exist or can't be activated.
pub fn with_endpoint<T>(
    device_id: Option<&str>,
    f: impl FnOnce(&IAudioEndpointVolume) -> Result<T>,
) -> Result<Option<T>> {
    let _com = ComGuard::new().map_err(|e| to_napi_err("failed to initialize COM", e))?;
    let Some(device) = resolve_device(device_id)? else {
        return Ok(None);
    };
    let Some(endpoint) =
        activate::<IAudioEndpointVolume>(&device, "failed to activate endpoint volume")?
    else {
        return Ok(None);
    };
    f(&endpoint).map(Some)
}

fn volume_range(endpoint: &IAudioEndpointVolume) -> Result<(f32, f32, f32)> {
    let (mut min, mut max, mut inc) = (0f32, 0f32, 0f32);
    unsafe { endpoint.GetVolumeRange(&mut min, &mut max, &mut inc) }
        .map_err(|e| to_napi_err("failed to read endpoint volume range", e))?;
    Ok((min, max, inc))
}

pub fn get_endpoint_volume(device_id: Option<&str>) -> Result<Option<EndpointVolume>> {
    with_endpoint(device_id, |ep| unsafe {
        let err = |e: windows::core::Error| to_napi_err("failed to read endpoint volume", e);
        let channels = (0..ep.GetChannelCount().map_err(err)?)
            .map(|i| {
                Ok(ChannelVolume {
                    volume: ep.GetChannelVolumeLevelScalar(i).map_err(err)? as f64,
                    volume_db: ep.GetChannelVolumeLevel(i).map_err(err)? as f64,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let (min_db, max_db, increment_db) = volume_range(ep)?;
        let (mut current, mut count) = (0u32, 0u32);
        ep.GetVolumeStepInfo(&mut current, &mut count)
            .map_err(err)?;
        Ok(EndpointVolume {
            volume: ep.GetMasterVolumeLevelScalar().map_err(err)? as f64,
            volume_db: ep.GetMasterVolumeLevel().map_err(err)? as f64,
            muted: ep.GetMute().map_err(err)?.as_bool(),
            channels,
            range: VolumeRange {
                min_db: min_db as f64,
                max_db: max_db as f64,
                increment_db: increment_db as f64,
            },
            step: VolumeStep { current, count },
            hardware_support: ep.QueryHardwareSupport().map_err(err)?,
        })
    })
}

#[napi(string_enum)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepDirection {
    #[napi(value = "up")]
    Up,
    #[napi(value = "down")]
    Down,
}

pub fn validate_db(db: f64, min_db: f32, max_db: f32) -> Result<()> {
    if !db.is_finite() || db < min_db as f64 || db > max_db as f64 {
        return Err(Error::new(
            Status::InvalidArg,
            format!("volumeDb must be between {min_db} and {max_db} dB, got {db}"),
        ));
    }
    Ok(())
}

pub fn validate_channel(channel: u32, count: u32) -> Result<()> {
    if channel >= count {
        return Err(Error::new(
            Status::InvalidArg,
            format!("channel {channel} is out of range; the device has {count} channel(s)"),
        ));
    }
    Ok(())
}

/// Setters report `true` when applied and `false` when the device doesn't exist.
fn applied(result: Result<Option<()>>) -> Result<bool> {
    result.map(|done| done.is_some())
}

pub fn set_volume(volume: f64, device_id: Option<&str>) -> Result<bool> {
    validate_volume(volume)?;
    applied(with_endpoint(device_id, |ep| {
        unsafe { ep.SetMasterVolumeLevelScalar(volume as f32, null()) }
            .map_err(|e| to_napi_err("failed to set endpoint volume", e))
    }))
}

pub fn set_volume_db(db: f64, device_id: Option<&str>) -> Result<bool> {
    // Reject NaN/Infinity up front so they throw even for an unknown device.
    if !db.is_finite() {
        return Err(Error::new(
            Status::InvalidArg,
            format!("volumeDb must be a finite number, got {db}"),
        ));
    }
    applied(with_endpoint(device_id, |ep| {
        let (min_db, max_db, _) = volume_range(ep)?;
        validate_db(db, min_db, max_db)?;
        unsafe { ep.SetMasterVolumeLevel(db as f32, null()) }
            .map_err(|e| to_napi_err("failed to set endpoint volume (dB)", e))
    }))
}

pub fn set_mute(muted: bool, device_id: Option<&str>) -> Result<bool> {
    applied(with_endpoint(device_id, |ep| {
        unsafe { ep.SetMute(muted, null()) }
            .map_err(|e| to_napi_err("failed to set endpoint mute", e))
    }))
}

pub fn set_channel_volume(channel: f64, volume: f64, device_id: Option<&str>) -> Result<bool> {
    let channel = validate_index("channel", channel)?;
    validate_volume(volume)?;
    applied(with_endpoint(device_id, |ep| {
        let count = unsafe { ep.GetChannelCount() }
            .map_err(|e| to_napi_err("failed to read endpoint channel count", e))?;
        validate_channel(channel, count)?;
        unsafe { ep.SetChannelVolumeLevelScalar(channel, volume as f32, null()) }
            .map_err(|e| to_napi_err("failed to set endpoint channel volume", e))
    }))
}

pub fn step(direction: StepDirection, device_id: Option<&str>) -> Result<bool> {
    applied(with_endpoint(device_id, |ep| {
        unsafe {
            match direction {
                StepDirection::Up => ep.VolumeStepUp(null()),
                StepDirection::Down => ep.VolumeStepDown(null()),
            }
        }
        .map_err(|e| to_napi_err("failed to step endpoint volume", e))
    }))
}

#[cfg(test)]
mod tests {
    use super::{validate_channel, validate_db};

    #[test]
    fn db_inside_range_is_ok() {
        assert!(validate_db(-10.0, -65.25, 0.0).is_ok());
        assert!(validate_db(-65.25, -65.25, 0.0).is_ok());
        assert!(validate_db(0.0, -65.25, 0.0).is_ok());
    }

    #[test]
    fn db_outside_range_or_non_finite_is_rejected() {
        assert!(validate_db(0.01, -65.25, 0.0).is_err());
        assert!(validate_db(-65.3, -65.25, 0.0).is_err());
        assert!(validate_db(f64::NAN, -65.25, 0.0).is_err());
        assert!(validate_db(f64::INFINITY, -65.25, 0.0).is_err());
    }

    #[test]
    fn channel_must_be_below_count() {
        assert!(validate_channel(0, 2).is_ok());
        assert!(validate_channel(1, 2).is_ok());
        assert!(validate_channel(2, 2).is_err());
        assert!(validate_channel(u32::MAX, 2).is_err());
        assert!(validate_channel(0, 0).is_err());
    }
}
