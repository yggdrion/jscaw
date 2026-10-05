use super::{gated_tsfn, subscribe, EventTsfn, Subscription};
use crate::com::{is_self_initiated, to_napi_err};
use crate::devices::com_guard;
use crate::sessions::{
    format_guid, matching_sessions, read_string, session_info, session_manager, AudioSession,
    SessionState, SessionTarget,
};
use napi::bindgen_prelude::{Either, Function, JsValuesTupleIntoVec, Null, Unknown};
use napi::threadsafe_function::ThreadsafeFunctionCallMode;
use napi::{Env, Result};
use napi_derive::napi;
use std::sync::Arc;
use windows::core::{implement, Interface, Ref, BOOL, GUID, PCWSTR};
use windows::Win32::Media::Audio::{
    AudioSessionDisconnectReason, AudioSessionState, DisconnectReasonDeviceRemoval,
    DisconnectReasonExclusiveModeOverride, DisconnectReasonFormatChanged,
    DisconnectReasonServerShutdown, DisconnectReasonSessionDisconnected,
    DisconnectReasonSessionLogoff, IAudioSessionControl, IAudioSessionControl2,
    IAudioSessionEvents, IAudioSessionEvents_Impl, IAudioSessionNotification,
    IAudioSessionNotification_Impl, IAudioVolumeDuckNotification,
    IAudioVolumeDuckNotification_Impl,
};

#[napi(string_enum)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisconnectReason {
    #[napi(value = "deviceRemoval")]
    DeviceRemoval,
    #[napi(value = "serverShutdown")]
    ServerShutdown,
    #[napi(value = "formatChanged")]
    FormatChanged,
    #[napi(value = "sessionLogoff")]
    SessionLogoff,
    #[napi(value = "sessionDisconnected")]
    SessionDisconnected,
    #[napi(value = "exclusiveModeOverride")]
    ExclusiveModeOverride,
}

impl DisconnectReason {
    fn from_windows(reason: AudioSessionDisconnectReason) -> Option<Self> {
        [
            (DisconnectReasonDeviceRemoval, Self::DeviceRemoval),
            (DisconnectReasonServerShutdown, Self::ServerShutdown),
            (DisconnectReasonFormatChanged, Self::FormatChanged),
            (DisconnectReasonSessionLogoff, Self::SessionLogoff),
            (
                DisconnectReasonSessionDisconnected,
                Self::SessionDisconnected,
            ),
            (
                DisconnectReasonExclusiveModeOverride,
                Self::ExclusiveModeOverride,
            ),
        ]
        .into_iter()
        .find_map(|(windows, ours)| (windows == reason).then_some(ours))
    }
}

/// pycaw's `AudioSessionEvents` callbacks as one tagged union, discriminated by `type`.
/// `instanceId` says which session fired, since one target can match several.
/// `channelVolumeChanged.changedChannel` is `null` when every channel changed.
#[napi(discriminant_case = "camelCase")]
#[derive(Debug)]
pub enum SessionEvent {
    DisplayNameChanged {
        instance_id: String,
        display_name: String,
        self_initiated: bool,
    },
    IconPathChanged {
        instance_id: String,
        icon_path: String,
        self_initiated: bool,
    },
    VolumeChanged {
        instance_id: String,
        volume: f64,
        muted: bool,
        self_initiated: bool,
    },
    ChannelVolumeChanged {
        instance_id: String,
        channel_volumes: Vec<f64>,
        changed_channel: Either<u32, Null>,
        self_initiated: bool,
    },
    GroupingChanged {
        instance_id: String,
        grouping_param: String,
        self_initiated: bool,
    },
    StateChanged {
        instance_id: String,
        state: SessionState,
    },
    Disconnected {
        instance_id: String,
        reason: DisconnectReason,
    },
}

/// pycaw's `IAudioVolumeDuckNotification`. `instanceId` is the communications session's
/// instance identifier, the same one `listSessions` reports.
#[napi(discriminant_case = "camelCase")]
#[derive(Debug)]
pub enum DuckEvent {
    Duck {
        instance_id: String,
        active_session_count: u32,
    },
    Unduck {
        instance_id: String,
    },
}

/// Windows passes `(DWORD)-1` when every channel changed.
fn changed_channel(channel: u32) -> Option<u32> {
    (channel != u32::MAX).then_some(channel)
}

fn emit<T: JsValuesTupleIntoVec + 'static>(tsfn: &EventTsfn<T>, event: Option<T>) {
    if let Some(event) = event {
        // Fails only once the env is closing, when nobody is listening anyway.
        tsfn.call(event, ThreadsafeFunctionCallMode::NonBlocking);
    }
}

/// # Safety
/// `s` is null or a valid, NUL-terminated wide string for the duration of the callback.
unsafe fn wide(s: &PCWSTR) -> String {
    if s.is_null() {
        return String::new();
    }
    s.to_string().unwrap_or_default()
}

/// # Safety
/// `context` is null or valid for the duration of the callback.
unsafe fn self_initiated(context: *const GUID) -> bool {
    context.as_ref().is_some_and(is_self_initiated)
}

/// Receives new-session callbacks on an audio worker thread and queues them to JS.
#[implement(IAudioSessionNotification)]
struct SessionCreatedNotifier {
    tsfn: EventTsfn<AudioSession>,
}

impl IAudioSessionNotification_Impl for SessionCreatedNotifier_Impl {
    fn OnSessionCreated(&self, session: Ref<IAudioSessionControl>) -> windows::core::Result<()> {
        let event = session
            .ok()
            .and_then(|s| s.cast::<IAudioSessionControl2>())
            .ok()
            .and_then(|control| {
                let pid = unsafe { control.GetProcessId() }.ok()?;
                session_info(&control, pid, true)
            });
        emit(&self.tsfn, event);
        Ok(())
    }
}

/// Receives one session's callbacks on an audio worker thread and queues them to JS.
#[implement(IAudioSessionEvents)]
struct SessionEventsNotifier {
    // Shared by every session the target matched; callbacks may run on several threads.
    tsfn: Arc<EventTsfn<SessionEvent>>,
    instance_id: String,
}

impl IAudioSessionEvents_Impl for SessionEventsNotifier_Impl {
    fn OnDisplayNameChanged(
        &self,
        name: &PCWSTR,
        context: *const GUID,
    ) -> windows::core::Result<()> {
        let event = unsafe {
            SessionEvent::DisplayNameChanged {
                instance_id: self.instance_id.clone(),
                display_name: wide(name),
                self_initiated: self_initiated(context),
            }
        };
        emit(&*self.tsfn, Some(event));
        Ok(())
    }

    fn OnIconPathChanged(&self, path: &PCWSTR, context: *const GUID) -> windows::core::Result<()> {
        let event = unsafe {
            SessionEvent::IconPathChanged {
                instance_id: self.instance_id.clone(),
                icon_path: wide(path),
                self_initiated: self_initiated(context),
            }
        };
        emit(&*self.tsfn, Some(event));
        Ok(())
    }

    fn OnSimpleVolumeChanged(
        &self,
        volume: f32,
        muted: BOOL,
        context: *const GUID,
    ) -> windows::core::Result<()> {
        let event = SessionEvent::VolumeChanged {
            instance_id: self.instance_id.clone(),
            volume: volume.into(),
            muted: muted.as_bool(),
            self_initiated: unsafe { self_initiated(context) },
        };
        emit(&*self.tsfn, Some(event));
        Ok(())
    }

    fn OnChannelVolumeChanged(
        &self,
        count: u32,
        volumes: *const f32,
        changed: u32,
        context: *const GUID,
    ) -> windows::core::Result<()> {
        // SAFETY: Windows passes `count` volumes, valid for the duration of the call.
        let channel_volumes = if volumes.is_null() {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(volumes, count as usize) }
                .iter()
                .copied()
                .map(f64::from)
                .collect()
        };
        let event = SessionEvent::ChannelVolumeChanged {
            instance_id: self.instance_id.clone(),
            channel_volumes,
            changed_channel: changed_channel(changed).map_or(Either::B(Null), Either::A),
            self_initiated: unsafe { self_initiated(context) },
        };
        emit(&*self.tsfn, Some(event));
        Ok(())
    }

    fn OnGroupingParamChanged(
        &self,
        param: *const GUID,
        context: *const GUID,
    ) -> windows::core::Result<()> {
        let event = unsafe {
            SessionEvent::GroupingChanged {
                instance_id: self.instance_id.clone(),
                grouping_param: param.as_ref().copied().map(format_guid).unwrap_or_default(),
                self_initiated: self_initiated(context),
            }
        };
        emit(&*self.tsfn, Some(event));
        Ok(())
    }

    fn OnStateChanged(&self, state: AudioSessionState) -> windows::core::Result<()> {
        let event = SessionState::from_windows(state).map(|state| SessionEvent::StateChanged {
            instance_id: self.instance_id.clone(),
            state,
        });
        emit(&*self.tsfn, event);
        Ok(())
    }

    fn OnSessionDisconnected(
        &self,
        reason: AudioSessionDisconnectReason,
    ) -> windows::core::Result<()> {
        let event =
            DisconnectReason::from_windows(reason).map(|reason| SessionEvent::Disconnected {
                instance_id: self.instance_id.clone(),
                reason,
            });
        emit(&*self.tsfn, event);
        Ok(())
    }
}

/// Receives ducking callbacks on an audio worker thread and queues them to JS.
#[implement(IAudioVolumeDuckNotification)]
struct DuckNotifier {
    tsfn: EventTsfn<DuckEvent>,
}

impl IAudioVolumeDuckNotification_Impl for DuckNotifier_Impl {
    fn OnVolumeDuckNotification(&self, id: &PCWSTR, count: u32) -> windows::core::Result<()> {
        let event = DuckEvent::Duck {
            instance_id: unsafe { wide(id) },
            active_session_count: count,
        };
        emit(&self.tsfn, Some(event));
        Ok(())
    }

    fn OnVolumeUnduckNotification(&self, id: &PCWSTR) -> windows::core::Result<()> {
        let event = DuckEvent::Unduck {
            instance_id: unsafe { wide(id) },
        };
        emit(&self.tsfn, Some(event));
        Ok(())
    }
}

/// `Ok(None)` when the device doesn't exist or can't be activated.
pub fn on_session_created<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
    device_id: Option<&str>,
) -> Result<Option<Function<'e, (), ()>>> {
    let com = com_guard()?;
    let Some(manager) = session_manager(device_id)? else {
        return Ok(None);
    };
    let (tsfn, active) = gated_tsfn(env, "onSessionCreated", callback)?;
    let notifier: IAudioSessionNotification = SessionCreatedNotifier { tsfn }.into();
    unsafe {
        manager
            .RegisterSessionNotification(&notifier)
            .map_err(|e| to_napi_err("failed to register session notifications", e))?;
        // Windows only starts sending new-session notifications once the manager has
        // enumerated its sessions at least once.
        let _ = manager.GetSessionEnumerator();
    }
    subscribe(
        env,
        Subscription::new(com, move || unsafe {
            active.set(false);
            // Failing here only means the device is gone; dropping `notifier` releases the
            // threadsafe function either way.
            let _ = manager.UnregisterSessionNotification(&notifier);
        }),
    )
    .map(Some)
}

/// Binds to the sessions matching `target` now; `Ok(None)` when none do.
pub fn on_session_event<'e>(
    env: &'e Env,
    target: SessionTarget,
    callback: Function<Unknown<'static>, ()>,
) -> Result<Option<Function<'e, (), ()>>> {
    // Taken first: it keeps COM initialized for as long as the controls below live.
    let com = com_guard()?;
    let controls = matching_sessions(target)?;
    if controls.is_empty() {
        return Ok(None);
    }
    let (tsfn, active) = gated_tsfn(env, "onSessionEvent", callback)?;
    let tsfn = Arc::new(tsfn);
    let mut registered: Vec<(IAudioSessionControl2, IAudioSessionEvents)> = Vec::new();
    for control in controls {
        let instance_id = read_string(unsafe { control.GetSessionInstanceIdentifier() });
        let notifier: IAudioSessionEvents = SessionEventsNotifier {
            tsfn: Arc::clone(&tsfn),
            instance_id,
        }
        .into();
        // A session that expired since it matched is skipped, like the setters skip it.
        if unsafe { control.RegisterAudioSessionNotification(&notifier) }.is_ok() {
            registered.push((control, notifier));
        }
    }
    if registered.is_empty() {
        return Ok(None);
    }
    subscribe(
        env,
        Subscription::new(com, move || {
            active.set(false);
            for (control, notifier) in registered {
                let _ = unsafe { control.UnregisterAudioSessionNotification(&notifier) };
            }
            drop(tsfn);
        }),
    )
    .map(Some)
}

/// Ducking notifications for every session on the device. `Ok(None)` when the device
/// doesn't exist or can't be activated.
pub fn on_duck_event<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
    device_id: Option<&str>,
) -> Result<Option<Function<'e, (), ()>>> {
    let com = com_guard()?;
    let Some(manager) = session_manager(device_id)? else {
        return Ok(None);
    };
    let (tsfn, active) = gated_tsfn(env, "onDuckEvent", callback)?;
    let notifier: IAudioVolumeDuckNotification = DuckNotifier { tsfn }.into();
    // A null session id asks for every session's duck notifications.
    unsafe { manager.RegisterDuckNotification(PCWSTR::null(), &notifier) }
        .map_err(|e| to_napi_err("failed to register duck notifications", e))?;
    subscribe(
        env,
        Subscription::new(com, move || unsafe {
            active.set(false);
            let _ = manager.UnregisterDuckNotification(&notifier);
        }),
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_disconnect_reason() {
        for (windows, ours) in [
            (
                DisconnectReasonDeviceRemoval,
                DisconnectReason::DeviceRemoval,
            ),
            (
                DisconnectReasonServerShutdown,
                DisconnectReason::ServerShutdown,
            ),
            (
                DisconnectReasonFormatChanged,
                DisconnectReason::FormatChanged,
            ),
            (
                DisconnectReasonSessionLogoff,
                DisconnectReason::SessionLogoff,
            ),
            (
                DisconnectReasonSessionDisconnected,
                DisconnectReason::SessionDisconnected,
            ),
            (
                DisconnectReasonExclusiveModeOverride,
                DisconnectReason::ExclusiveModeOverride,
            ),
        ] {
            assert_eq!(DisconnectReason::from_windows(windows), Some(ours));
        }
        assert_eq!(
            DisconnectReason::from_windows(AudioSessionDisconnectReason(9)),
            None
        );
    }

    #[test]
    fn changed_channel_all_is_none() {
        assert_eq!(changed_channel(u32::MAX), None);
        assert_eq!(changed_channel(2), Some(2));
    }

    #[test]
    fn null_event_context_is_not_self_initiated() {
        assert!(!unsafe { self_initiated(std::ptr::null()) });
        assert!(unsafe { self_initiated(crate::com::event_context()) });
    }
}
