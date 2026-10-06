use napi::{Error, Result, Status};
use std::sync::OnceLock;
use windows::core::{Interface, GUID, HRESULT, HSTRING, PWSTR};
use windows::Win32::Foundation::{E_INVALIDARG, RPC_E_CHANGED_MODE};
use windows::Win32::Media::Audio::{
    eConsole, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

pub fn validate_volume(volume: f64) -> Result<()> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("volume must be a finite number between 0 and 1, got {volume}"),
        ));
    }
    Ok(())
}

/// JS numbers arrive as `f64`: napi's `u32` conversion silently turns NaN into 0 and -1 into
/// `u32::MAX`, so integer arguments are checked here.
pub fn validate_index(name: &str, value: f64) -> Result<u32> {
    if value.fract() != 0.0 || !(0.0..=u32::MAX as f64).contains(&value) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("{name} must be a non-negative integer, got {value}"),
        ));
    }
    Ok(value as u32)
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, uppercase: how Windows (and pycaw) print GUIDs.
pub fn format_guid(guid: &GUID) -> String {
    format!("{{{guid:?}}}")
}

pub fn to_napi_err(context: &str, err: windows::core::Error) -> Error {
    Error::new(
        Status::GenericFailure,
        format!(
            "{context}: {} (0x{:08X})",
            err.message(),
            err.code().0 as u32
        ),
    )
}

/// Random per process (pycaw magic's `uuid4` trick): every jscaw setter passes it as the
/// change's event context, so notifications can tell our own changes from external ones.
pub fn event_context() -> *const GUID {
    static EVENT_CONTEXT: OnceLock<GUID> = OnceLock::new();
    EVENT_CONTEXT.get_or_init(|| GUID::new().unwrap_or_default())
}

/// True when a notification's event context is ours, i.e. a jscaw setter in this process made it.
pub fn is_self_initiated(context: &GUID) -> bool {
    *context == unsafe { *event_context() }
}

/// RAII guard: initializes COM (MTA) for the calling thread on construction and
/// uninitializes on drop. Synchronous napi calls run on the JS thread, so this pairs one
/// init/uninit per call on that thread. A thread that is already STA (e.g. Electron's main
/// thread) returns `RPC_E_CHANGED_MODE`: COM is usable there, but the init isn't ours to undo.
pub struct ComGuard {
    owns_init: bool,
}

impl ComGuard {
    pub fn new() -> windows::core::Result<Self> {
        match unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
            Ok(()) => Ok(Self { owns_init: true }),
            Err(e) if e.code() == RPC_E_CHANGED_MODE => Ok(Self { owns_init: false }),
            Err(e) => Err(e),
        }
    }
}

pub fn com_guard() -> Result<ComGuard> {
    ComGuard::new().map_err(|e| to_napi_err("failed to initialize COM", e))
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.owns_init {
            unsafe { CoUninitialize() };
        }
    }
}

/// Returned when a device doesn't exist: no default endpoint (e.g. a headless CI runner) or
/// an unknown device id. A legitimate "nothing there" state, not a failure.
pub const ERROR_NOT_FOUND_HRESULT: i32 = 0x8007_0490_u32 as i32;

pub fn device_enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|e| to_napi_err("failed to create audio device enumerator", e))
}

/// Maps "device not found" (plus any `also_missing` codes) to `Ok(None)`; every other
/// failure becomes a napi error.
fn missing_as_none(
    result: windows::core::Result<IMMDevice>,
    context: &str,
    also_missing: &[HRESULT],
) -> Result<Option<IMMDevice>> {
    match result {
        Ok(device) => Ok(Some(device)),
        Err(e) if e.code().0 == ERROR_NOT_FOUND_HRESULT || also_missing.contains(&e.code()) => {
            Ok(None)
        }
        Err(e) => Err(to_napi_err(context, e)),
    }
}

pub fn default_device(flow: EDataFlow, role: ERole) -> Result<Option<IMMDevice>> {
    let enumerator = device_enumerator()?;
    missing_as_none(
        unsafe { enumerator.GetDefaultAudioEndpoint(flow, role) },
        "failed to get default audio endpoint",
        &[],
    )
}

/// `None` → the default render/console endpoint, matching the pre-device-aware behaviour.
pub fn resolve_device(device_id: Option<&str>) -> Result<Option<IMMDevice>> {
    match device_id {
        None => default_device(eRender, eConsole),
        Some(id) => {
            let enumerator = device_enumerator()?;
            missing_as_none(
                unsafe { enumerator.GetDevice(&HSTRING::from(id)) },
                "failed to get audio device",
                // A malformed id (e.g. "" or arbitrary text) is rejected with E_INVALIDARG
                // rather than E_NOTFOUND; to the caller it's equally "no such device".
                &[E_INVALIDARG],
            )
        }
    }
}

/// `AUDCLNT_E_DEVICE_INVALIDATED`: returned when activating a disabled, unplugged or
/// not-present endpoint. To the caller that's "no usable device", like not-found.
const DEVICE_INVALIDATED_HRESULT: i32 = 0x8889_0004_u32 as i32;
/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)` / `ERROR_PATH_NOT_FOUND`: what activating or
/// enumerating a not-present (removed) endpoint actually returns in practice.
const FILE_NOT_FOUND_HRESULT: i32 = 0x8007_0002_u32 as i32;
const PATH_NOT_FOUND_HRESULT: i32 = 0x8007_0003_u32 as i32;

/// True for the errors a disabled, unplugged or removed endpoint returns. To the caller
/// that's "no usable device", not a failure.
pub fn is_missing_device(err: &windows::core::Error) -> bool {
    [
        DEVICE_INVALIDATED_HRESULT,
        ERROR_NOT_FOUND_HRESULT,
        FILE_NOT_FOUND_HRESULT,
        PATH_NOT_FOUND_HRESULT,
    ]
    .contains(&err.code().0)
}

/// Activates interface `T` on `device`. `Ok(None)` when the endpoint is disabled, unplugged
/// or gone.
pub fn activate<T: Interface>(device: &IMMDevice, context: &str) -> Result<Option<T>> {
    match unsafe { device.Activate::<T>(CLSCTX_ALL, None) } {
        Ok(interface) => Ok(Some(interface)),
        Err(e) if is_missing_device(&e) => Ok(None),
        Err(e) => Err(to_napi_err(context, e)),
    }
}

/// Converts and frees a string that a COM getter allocated with `CoTaskMemAlloc`.
///
/// # Safety
/// `raw` must be null or a `CoTaskMemAlloc`'d, NUL-terminated wide string the caller owns.
pub unsafe fn take_co_string(raw: PWSTR) -> String {
    if raw.is_null() {
        return String::new();
    }
    let value = String::from_utf16_lossy(raw.as_wide());
    CoTaskMemFree(Some(raw.0 as *const _));
    value
}

#[cfg(test)]
mod tests {
    use super::{event_context, is_self_initiated, validate_index, validate_volume, ComGuard};
    use windows::core::GUID;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    #[test]
    fn com_guard_owns_a_fresh_mta_init() {
        std::thread::spawn(|| assert!(ComGuard::new().unwrap().owns_init))
            .join()
            .unwrap();
    }

    #[test]
    fn com_guard_tolerates_an_sta_thread() {
        std::thread::spawn(|| unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let guard = ComGuard::new().unwrap();
            assert!(!guard.owns_init);
            drop(guard);
            CoUninitialize();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn only_our_event_context_is_self_initiated() {
        assert!(is_self_initiated(unsafe { &*event_context() }));
        assert!(!is_self_initiated(&GUID::zeroed()));
        assert!(!is_self_initiated(&GUID::from_u128(1)));
    }

    #[test]
    fn validate_index_names_the_argument() {
        assert_eq!(validate_index("channel", 2.0).unwrap(), 2);
        let err = validate_index("channel", f64::NAN).unwrap_err();
        assert!(err.reason.contains("channel must be"), "{}", err.reason);
    }

    #[test]
    fn accepts_mid_range_volume() {
        assert!(validate_volume(0.5).is_ok());
    }

    #[test]
    fn rejects_below_zero() {
        assert!(validate_volume(-0.01).is_err());
    }

    #[test]
    fn rejects_above_one() {
        assert!(validate_volume(1.01).is_err());
    }
}
