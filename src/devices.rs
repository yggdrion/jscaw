use crate::com::{
    default_device, device_enumerator, resolve_device, take_co_string, to_napi_err, ComGuard,
};
use napi::bindgen_prelude::Either3;
use napi::Result;
use napi_derive::napi;
use std::collections::HashMap;
use windows::core::{Interface, GUID};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eAll, eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice,
    IMMEndpoint, DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT,
    DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::STGM_READ;
use windows::Win32::System::Variant::{VT_BOOL, VT_CLSID, VT_I4, VT_LPWSTR, VT_UI4, VT_UI8};

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

    pub(crate) fn from_windows(flow: EDataFlow) -> Option<Self> {
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

    pub(crate) fn from_windows(state: DEVICE_STATE) -> Option<Self> {
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
    pub(crate) fn to_windows(self) -> ERole {
        match self {
            Self::Console => eConsole,
            Self::Multimedia => eMultimedia,
            Self::Communications => eCommunications,
        }
    }

    pub(crate) fn from_windows(role: ERole) -> Option<Self> {
        [Self::Console, Self::Multimedia, Self::Communications]
            .into_iter()
            .find(|r| r.to_windows() == role)
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
        let id = take_co_string(device.GetId().ok()?);
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

/// A decoded property value; unsupported PROPVARIANT types become `None` (JS `null`).
pub type PropertyValue = Either3<String, f64, bool>;

fn braced_guid(guid: &GUID) -> String {
    format!("{{{guid:?}}}")
}

/// pycaw's `str(PROPERTYKEY)`: `"{FMTID} pid"` with an uppercase, braced GUID.
pub(crate) fn property_key_name(key: &PROPERTYKEY) -> String {
    format!("{} {}", braced_guid(&key.fmtid), key.pid)
}

fn decode(value: &PROPVARIANT) -> Option<PropertyValue> {
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        let data = &inner.Anonymous;
        match inner.vt {
            VT_LPWSTR if data.pwszVal.is_null() => Some(Either3::A(String::new())),
            VT_LPWSTR => Some(Either3::A(String::from_utf16_lossy(data.pwszVal.as_wide()))),
            VT_BOOL => Some(Either3::C(data.boolVal.as_bool())),
            VT_UI4 => Some(Either3::B(data.ulVal.into())),
            VT_I4 => Some(Either3::B(data.lVal.into())),
            // ponytail: VT_UI8 > 2^53 loses precision as a JS number; switch to BigInt if a
            // real property needs it.
            VT_UI8 => Some(Either3::B(data.uhVal as f64)),
            VT_CLSID => data.puuid.as_ref().map(|g| Either3::A(braced_guid(g))),
            _ => None,
        }
    }
}

pub(crate) fn com_guard() -> Result<ComGuard> {
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
    // Bind to a local so the device is released before `_com` uninitializes COM; a tail
    // expression temporary would outlive the guard under edition 2021 drop order.
    let device = default_device(flow, role)?;
    Ok(device.as_ref().and_then(to_device))
}

pub fn get_device(id: String) -> Result<Option<Device>> {
    let _com = com_guard()?;
    let device = resolve_device(Some(&id))?;
    Ok(device.as_ref().and_then(to_device))
}

pub fn get_device_properties(id: String) -> Result<Option<HashMap<String, Option<PropertyValue>>>> {
    let _com = com_guard()?;
    let Some(device) = resolve_device(Some(&id))? else {
        return Ok(None);
    };
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }
        .map_err(|e| to_napi_err("failed to open device property store", e))?;
    let count = unsafe { store.GetCount() }
        .map_err(|e| to_napi_err("failed to get device property count", e))?;
    let mut props = HashMap::new();
    for i in 0..count {
        let mut key = PROPERTYKEY::default();
        // Like pycaw, a key or value that fails to read is skipped.
        if unsafe { store.GetAt(i, &mut key) }.is_err() {
            continue;
        }
        let Ok(value) = (unsafe { store.GetValue(&key) }) else {
            continue;
        };
        props.insert(property_key_name(&key), decode(&value));
    }
    Ok(Some(props))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;
    use windows::core::{GUID, PWSTR};
    use windows::Win32::Media::Audio::DEVICE_STATEMASK_ALL;
    use windows::Win32::System::Com::StructuredStorage::{
        PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    };
    use windows::Win32::System::Variant::{VARENUM, VT_CLSID, VT_LPWSTR};

    /// Borrows `field` into a PROPVARIANT; `ManuallyDrop` so it's never `PropVariantClear`ed.
    fn raw(vt: VARENUM, field: PROPVARIANT_0_0_0) -> ManuallyDrop<PROPVARIANT> {
        ManuallyDrop::new(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: field,
                }),
            },
        })
    }

    #[test]
    fn property_key_name_matches_pycaw() {
        assert_eq!(
            property_key_name(&PKEY_Device_FriendlyName),
            "{A45C254E-DF1C-4EFD-8020-67D146A850E0} 14"
        );
    }

    #[test]
    fn decode_handles_supported_types() {
        let num = |v: &PROPVARIANT| match decode(v) {
            Some(Either3::B(n)) => n,
            _ => panic!("expected number"),
        };
        assert_eq!(num(&PROPVARIANT::from(7u32)), 7.0);
        assert_eq!(num(&PROPVARIANT::from(-3i32)), -3.0);
        assert_eq!(num(&PROPVARIANT::from(1u64 << 40)), (1u64 << 40) as f64);
        assert!(matches!(
            decode(&PROPVARIANT::from(true)),
            Some(Either3::C(true))
        ));
        assert!(matches!(
            decode(&PROPVARIANT::from(false)),
            Some(Either3::C(false))
        ));

        let mut wide: Vec<u16> = "Speakers".encode_utf16().chain([0]).collect();
        let s = raw(
            VT_LPWSTR,
            PROPVARIANT_0_0_0 {
                pwszVal: PWSTR(wide.as_mut_ptr()),
            },
        );
        assert!(matches!(decode(&s), Some(Either3::A(ref v)) if v == "Speakers"));

        let mut guid = GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e);
        let clsid = raw(VT_CLSID, PROPVARIANT_0_0_0 { puuid: &mut guid });
        assert!(
            matches!(decode(&clsid), Some(Either3::A(ref v)) if v == "{1DA5D803-D492-4EDD-8C23-E0C0FFEE7F0E}")
        );
        let null_clsid = raw(
            VT_CLSID,
            PROPVARIANT_0_0_0 {
                puuid: std::ptr::null_mut(),
            },
        );
        assert!(decode(&null_clsid).is_none());
    }

    #[test]
    fn decode_returns_none_for_other_types() {
        assert!(decode(&PROPVARIANT::default()).is_none());
        assert!(decode(&PROPVARIANT::from(1.5f64)).is_none());
    }

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
