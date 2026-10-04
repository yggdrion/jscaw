use crate::com::{default_device, device_enumerator, resolve_device, to_napi_err, ComGuard};
use napi::Result;
use napi_derive::napi;
use windows::core::Interface;
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eAll, eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice,
    IMMEndpoint, DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT,
    DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::System::Com::{CoTaskMemFree, STGM_READ};

#[napi(string_enum)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceFlow {
    #[napi(value = "render")]
    Render,
    #[napi(value = "capture")]
    Capture,
}

#[napi(string_enum)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowFilter {
    #[napi(value = "render")]
    Render,
    #[napi(value = "capture")]
    Capture,
    #[napi(value = "all")]
    All,
}

#[napi(string_enum)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceState {
    #[napi(value = "active")]
    Active,
    #[napi(value = "disabled")]
    Disabled,
    #[napi(value = "notPresent")]
    NotPresent,
    #[napi(value = "unplugged")]
    Unplugged,
}

#[napi(string_enum)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceRole {
    #[napi(value = "console")]
    Console,
    #[napi(value = "multimedia")]
    Multimedia,
    #[napi(value = "communications")]
    Communications,
}

#[napi(object)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub flow: DeviceFlow,
    pub state: DeviceState,
}

#[napi(object)]
pub struct ListDevicesOptions {
    pub flow: Option<FlowFilter>,
    pub state: Option<Vec<DeviceState>>,
}

impl DeviceFlow {
    fn to_windows(self) -> EDataFlow {
        match self {
            Self::Render => eRender,
            Self::Capture => eCapture,
        }
    }

    fn from_windows(flow: EDataFlow) -> Option<Self> {
        match flow {
            f if f == eRender => Some(Self::Render),
            f if f == eCapture => Some(Self::Capture),
            _ => None,
        }
    }
}

impl FlowFilter {
    fn to_windows(self) -> EDataFlow {
        match self {
            Self::Render => eRender,
            Self::Capture => eCapture,
            Self::All => eAll,
        }
    }
}

impl DeviceState {
    fn bit(self) -> u32 {
        match self {
            Self::Active => DEVICE_STATE_ACTIVE.0,
            Self::Disabled => DEVICE_STATE_DISABLED.0,
            Self::NotPresent => DEVICE_STATE_NOTPRESENT.0,
            Self::Unplugged => DEVICE_STATE_UNPLUGGED.0,
        }
    }

    fn from_windows(state: DEVICE_STATE) -> Option<Self> {
        [
            Self::Active,
            Self::Disabled,
            Self::NotPresent,
            Self::Unplugged,
        ]
        .into_iter()
        .find(|s| s.bit() == state.0)
    }
}

impl DeviceRole {
    fn to_windows(self) -> ERole {
        match self {
            Self::Console => eConsole,
            Self::Multimedia => eMultimedia,
            Self::Communications => eCommunications,
        }
    }
}

fn state_mask(states: &[DeviceState]) -> u32 {
    states.iter().fold(0, |mask, s| mask | s.bit())
}

/// Reads a string property; empty when the store can't be opened or the value is missing.
fn string_property(device: &IMMDevice, key: &PROPERTYKEY) -> String {
    unsafe {
        device
            .OpenPropertyStore(STGM_READ)
            .and_then(|store| store.GetValue(key))
            .map(|value| value.to_string())
            .unwrap_or_default()
    }
}

/// Converts an `IMMDevice` into a `Device`. `None` if its id, flow or state can't be read,
/// so one broken device never fails a whole listing.
fn to_device(device: &IMMDevice) -> Option<Device> {
    unsafe {
        let raw_id = device.GetId().ok()?;
        let id = String::from_utf16_lossy(raw_id.as_wide());
        // GetId allocates the string with CoTaskMemAlloc; the caller owns it.
        CoTaskMemFree(Some(raw_id.0 as *const _));
        let endpoint = device.cast::<IMMEndpoint>().ok()?;
        let flow = DeviceFlow::from_windows(endpoint.GetDataFlow().ok()?)?;
        let state = DeviceState::from_windows(device.GetState().ok()?)?;
        let mut name = string_property(device, &PKEY_Device_FriendlyName);
        if name.is_empty() {
            name = string_property(device, &PKEY_Device_DeviceDesc);
        }
        Some(Device {
            id,
            name,
            flow,
            state,
        })
    }
}

fn com_guard() -> Result<ComGuard> {
    ComGuard::new().map_err(|e| to_napi_err("failed to initialize COM", e))
}

pub fn list_devices(options: Option<ListDevicesOptions>) -> Result<Vec<Device>> {
    let options = options.unwrap_or(ListDevicesOptions {
        flow: None,
        state: None,
    });
    let flow = options.flow.unwrap_or(FlowFilter::All).to_windows();
    let mask = state_mask(&options.state.unwrap_or_else(|| vec![DeviceState::Active]));
    if mask == 0 {
        return Ok(Vec::new());
    }
    let _com = com_guard()?;
    let enumerator = device_enumerator()?;
    unsafe {
        let collection = enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE(mask))
            .map_err(|e| to_napi_err("failed to enumerate audio devices", e))?;
        let count = collection
            .GetCount()
            .map_err(|e| to_napi_err("failed to get audio device count", e))?;
        Ok((0..count)
            .filter_map(|i| collection.Item(i).ok())
            .filter_map(|device| to_device(&device))
            .collect())
    }
}

pub fn get_default_device(
    flow: Option<DeviceFlow>,
    role: Option<DeviceRole>,
) -> Result<Option<Device>> {
    let _com = com_guard()?;
    let flow = flow.unwrap_or(DeviceFlow::Render).to_windows();
    let role = role.unwrap_or(DeviceRole::Console).to_windows();
    Ok(default_device(flow, role)?.as_ref().and_then(to_device))
}

pub fn get_device(id: String) -> Result<Option<Device>> {
    let _com = com_guard()?;
    Ok(resolve_device(Some(&id))?.as_ref().and_then(to_device))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::Audio::DEVICE_STATEMASK_ALL;

    #[test]
    fn state_mask_ors_requested_states() {
        assert_eq!(state_mask(&[DeviceState::Active]), 1);
        assert_eq!(
            state_mask(&[DeviceState::Active, DeviceState::Unplugged]),
            9
        );
        assert_eq!(
            state_mask(&[
                DeviceState::Active,
                DeviceState::Disabled,
                DeviceState::NotPresent,
                DeviceState::Unplugged
            ]),
            DEVICE_STATEMASK_ALL
        );
        assert_eq!(state_mask(&[]), 0);
    }

    #[test]
    fn device_state_round_trips_through_windows_constants() {
        for state in [
            DeviceState::Active,
            DeviceState::Disabled,
            DeviceState::NotPresent,
            DeviceState::Unplugged,
        ] {
            assert_eq!(
                DeviceState::from_windows(DEVICE_STATE(state_mask(&[state]))),
                Some(state)
            );
        }
        assert_eq!(DeviceState::from_windows(DEVICE_STATE(0)), None);
    }

    #[test]
    fn flow_maps_both_ways() {
        assert_eq!(DeviceFlow::from_windows(eRender), Some(DeviceFlow::Render));
        assert_eq!(
            DeviceFlow::from_windows(eCapture),
            Some(DeviceFlow::Capture)
        );
        assert_eq!(DeviceFlow::from_windows(eAll), None);
        assert_eq!(FlowFilter::All.to_windows(), eAll);
        assert_eq!(DeviceFlow::Capture.to_windows(), eCapture);
    }
}
