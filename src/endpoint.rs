use crate::com::{resolve_device, to_napi_err, ComGuard, ERROR_NOT_FOUND_HRESULT};
use napi::Result;
use napi_derive::napi;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::System::Com::CLSCTX_ALL;

/// `AUDCLNT_E_DEVICE_INVALIDATED`: returned when activating a disabled, unplugged or
/// not-present endpoint. To the caller that's "no usable device", like not-found.
const DEVICE_INVALIDATED_HRESULT: i32 = 0x8889_0004_u32 as i32;
/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`: what activating a not-present (removed) endpoint
/// actually returns in practice.
const FILE_NOT_FOUND_HRESULT: i32 = 0x8007_0002_u32 as i32;

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
    let endpoint = match unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) } {
        Ok(endpoint) => endpoint,
        Err(e)
            if [
                DEVICE_INVALIDATED_HRESULT,
                ERROR_NOT_FOUND_HRESULT,
                FILE_NOT_FOUND_HRESULT,
            ]
            .contains(&e.code().0) =>
        {
            return Ok(None)
        }
        Err(e) => return Err(to_napi_err("failed to activate endpoint volume", e)),
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
