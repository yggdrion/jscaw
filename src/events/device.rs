use super::{subscribe, Subscription};
use crate::com::{device_enumerator, to_napi_err};
use crate::devices::{com_guard, property_key_name, DeviceFlow, DeviceRole, DeviceState};
use napi::bindgen_prelude::{Either, Function, Null, Unknown};
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::{Env, Result};
use napi_derive::napi;
use std::cell::Cell;
use std::rc::Rc;
use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    EDataFlow, ERole, IMMNotificationClient, IMMNotificationClient_Impl, DEVICE_STATE,
};

/// pycaw's `MMNotificationClient` callbacks as one tagged union, discriminated by `type`.
/// `defaultChanged.deviceId` is `null` when the flow/role no longer has a default device;
/// `propertyChanged.key` is `"{FMTID} pid"`, the same keys `getDeviceProperties` returns.
#[napi(discriminant_case = "camelCase")]
#[derive(Debug)]
pub enum DeviceEvent {
    Added {
        device_id: String,
    },
    Removed {
        device_id: String,
    },
    StateChanged {
        device_id: String,
        state: DeviceState,
    },
    DefaultChanged {
        flow: DeviceFlow,
        role: DeviceRole,
        device_id: Either<String, Null>,
    },
    PropertyChanged {
        device_id: String,
        key: String,
    },
}

/// `None` for a state jscaw doesn't model; such events are dropped.
fn state_changed(device_id: String, state: DEVICE_STATE) -> Option<DeviceEvent> {
    Some(DeviceEvent::StateChanged {
        device_id,
        state: DeviceState::from_windows(state)?,
    })
}

/// `None` for flows/roles jscaw doesn't model (e.g. `eAll`).
fn default_changed(flow: EDataFlow, role: ERole, device_id: Option<String>) -> Option<DeviceEvent> {
    Some(DeviceEvent::DefaultChanged {
        flow: DeviceFlow::from_windows(flow)?,
        role: DeviceRole::from_windows(role)?,
        device_id: device_id.map_or(Either::B(Null), Either::A),
    })
}

fn property_changed(device_id: String, key: &PROPERTYKEY) -> DeviceEvent {
    DeviceEvent::PropertyChanged {
        device_id,
        key: property_key_name(key),
    }
}

type DeviceTsfn = ThreadsafeFunction<DeviceEvent, (), DeviceEvent, napi::Status, false>;

/// Receives MMDevAPI callbacks on its own worker thread and queues them to JS.
#[implement(IMMNotificationClient)]
struct DeviceNotifier {
    tsfn: DeviceTsfn,
}

impl DeviceNotifier {
    fn emit(&self, event: Option<DeviceEvent>) {
        if let Some(event) = event {
            // Fails only once the env is closing, when nobody is listening anyway.
            self.tsfn
                .call(event, ThreadsafeFunctionCallMode::NonBlocking);
        }
    }
}

/// # Safety
/// `id` is a valid, NUL-terminated wide string for the duration of the callback.
unsafe fn id_string(id: &PCWSTR) -> String {
    id.to_string().unwrap_or_default()
}

impl IMMNotificationClient_Impl for DeviceNotifier_Impl {
    fn OnDeviceStateChanged(&self, id: &PCWSTR, state: DEVICE_STATE) -> windows::core::Result<()> {
        self.emit(state_changed(unsafe { id_string(id) }, state));
        Ok(())
    }

    fn OnDeviceAdded(&self, id: &PCWSTR) -> windows::core::Result<()> {
        let device_id = unsafe { id_string(id) };
        self.emit(Some(DeviceEvent::Added { device_id }));
        Ok(())
    }

    fn OnDeviceRemoved(&self, id: &PCWSTR) -> windows::core::Result<()> {
        let device_id = unsafe { id_string(id) };
        self.emit(Some(DeviceEvent::Removed { device_id }));
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        id: &PCWSTR,
    ) -> windows::core::Result<()> {
        let device_id = (!id.is_null()).then(|| unsafe { id_string(id) });
        self.emit(default_changed(flow, role, device_id));
        Ok(())
    }

    fn OnPropertyValueChanged(&self, id: &PCWSTR, key: &PROPERTYKEY) -> windows::core::Result<()> {
        self.emit(Some(property_changed(unsafe { id_string(id) }, key)));
        Ok(())
    }
}

pub fn on_device_event<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
) -> Result<Function<'e, (), ()>> {
    let com = com_guard()?;
    let enumerator = device_enumerator()?;
    // Events already queued when `unsubscribe()` runs are still dispatched by Node, so the
    // threadsafe function targets this gate, which drops them once `active` is cleared.
    let active = Rc::new(Cell::new(true));
    let gate_active = active.clone();
    let user = callback.create_ref()?;
    let gate =
        env.create_function_from_closure::<DeviceEvent, (), _>("onDeviceEvent", move |ctx| {
            if gate_active.get() {
                user.borrow_back(ctx.env)?
                    .call(ctx.get::<Unknown<'static>>(0)?)?;
            }
            Ok(())
        })?;
    let tsfn: DeviceTsfn = gate.build_threadsafe_function().build()?;
    let client: IMMNotificationClient = DeviceNotifier { tsfn }.into();
    unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }
        .map_err(|e| to_napi_err("failed to register device notifications", e))?;
    subscribe(
        env,
        Subscription::new(com, move || unsafe {
            active.set(false);
            // Failing here only means it's already gone; dropping `client` releases the
            // threadsafe function either way.
            let _ = enumerator.UnregisterEndpointNotificationCallback(&client);
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::{
        eAll, eCapture, eCommunications, eConsole, DEVICE_STATE, DEVICE_STATE_UNPLUGGED,
    };

    #[test]
    fn maps_state_changes() {
        assert!(matches!(
            state_changed("x".into(), DEVICE_STATE_UNPLUGGED),
            Some(DeviceEvent::StateChanged { device_id, state: DeviceState::Unplugged }) if device_id == "x"
        ));
        assert!(state_changed("x".into(), DEVICE_STATE(0x99)).is_none());
    }

    #[test]
    fn maps_default_changes() {
        assert!(matches!(
            default_changed(eCapture, eCommunications, None),
            Some(DeviceEvent::DefaultChanged {
                flow: DeviceFlow::Capture,
                role: DeviceRole::Communications,
                device_id: Either::B(Null)
            })
        ));
        assert!(default_changed(eAll, eConsole, Some("x".into())).is_none());
    }

    #[test]
    fn maps_property_changes_to_pycaw_keys() {
        assert!(matches!(
            property_changed("x".into(), &PKEY_Device_FriendlyName),
            DeviceEvent::PropertyChanged { device_id, key }
                if device_id == "x" && key == "{A45C254E-DF1C-4EFD-8020-67D146A850E0} 14"
        ));
    }
}
