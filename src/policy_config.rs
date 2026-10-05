// COM method names (e.g. `SetDefaultEndpoint`) keep their Windows casing.
#![allow(non_snake_case)]

use crate::com::{resolve_device, to_napi_err};
use crate::devices::{com_guard, DeviceRole};
use napi::{Error, Result, Status};
use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, HSTRING, PCWSTR};
use windows::Win32::Media::Audio::ERole;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

/// Undocumented `IPolicyConfig` (Win7+), the interface the Windows sound control panel uses.
/// Only `SetDefaultEndpoint` is called; the `_slotN` methods just reserve their vtable
/// positions (GetMixFormat … SetPropertyValue) and must never be invoked.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn _slot0(&self) -> HRESULT;
    fn _slot1(&self) -> HRESULT;
    fn _slot2(&self) -> HRESULT;
    fn _slot3(&self) -> HRESULT;
    fn _slot4(&self) -> HRESULT;
    fn _slot5(&self) -> HRESULT;
    fn _slot6(&self) -> HRESULT;
    fn _slot7(&self) -> HRESULT;
    fn _slot8(&self) -> HRESULT;
    fn _slot9(&self) -> HRESULT;
    fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
}

/// `CPolicyConfigClient`.
const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// `false` when the id doesn't resolve to a device, matching the endpoint setters.
pub fn set_default_device(device_id: String, roles: Option<Vec<DeviceRole>>) -> Result<bool> {
    let roles = roles.unwrap_or_else(|| {
        vec![
            DeviceRole::Console,
            DeviceRole::Multimedia,
            DeviceRole::Communications,
        ]
    });
    if roles.is_empty() {
        return Err(Error::new(Status::InvalidArg, "roles must not be empty"));
    }
    let _com = com_guard()?;
    if resolve_device(Some(&device_id))?.is_none() {
        return Ok(false);
    }
    let policy: IPolicyConfig =
        unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }
            .map_err(|e| to_napi_err("failed to create policy config", e))?;
    let id = HSTRING::from(&device_id);
    for role in roles {
        unsafe { policy.SetDefaultEndpoint(PCWSTR(id.as_ptr()), role.to_windows()) }
            .ok()
            .map_err(|e| to_napi_err("failed to set default audio endpoint", e))?;
    }
    Ok(true)
}
