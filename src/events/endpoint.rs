use super::{gated_tsfn, subscribe, EventTsfn, Subscription};
use crate::com::{activate, com_guard, is_self_initiated, resolve_device, to_napi_err};
use napi::bindgen_prelude::{Function, Unknown};
use napi::threadsafe_function::ThreadsafeFunctionCallMode;
use napi::{Env, Result};
use napi_derive::napi;
use windows::core::{implement, GUID};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::AUDIO_VOLUME_NOTIFICATION_DATA;

/// pycaw's `AudioEndpointVolumeCallback` payload. `selfInitiated` is true when a jscaw setter
/// in this process made the change.
#[napi(object)]
#[derive(Debug)]
pub struct EndpointVolumeEvent {
    pub volume: f64,
    pub muted: bool,
    pub channel_volumes: Vec<f64>,
    pub self_initiated: bool,
}

fn volume_event(context: &GUID, muted: bool, volume: f32, channels: &[f32]) -> EndpointVolumeEvent {
    EndpointVolumeEvent {
        volume: volume.into(),
        muted,
        channel_volumes: channels.iter().copied().map(f64::from).collect(),
        self_initiated: is_self_initiated(context),
    }
}

/// Receives endpoint volume callbacks on an audio worker thread and queues them to JS.
#[implement(IAudioEndpointVolumeCallback)]
struct EndpointVolumeNotifier {
    tsfn: EventTsfn<EndpointVolumeEvent>,
}

impl IAudioEndpointVolumeCallback_Impl for EndpointVolumeNotifier_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        // SAFETY: Windows passes a valid struct for the duration of the call.
        // `afChannelVolumes` is a C flexible array member: it holds `nChannels` entries even
        // though the Rust type declares one.
        let event = unsafe {
            let Some(data) = data.as_ref() else {
                return Ok(());
            };
            let channels =
                std::slice::from_raw_parts(data.afChannelVolumes.as_ptr(), data.nChannels as usize);
            volume_event(
                &data.guidEventContext,
                data.bMuted.as_bool(),
                data.fMasterVolume,
                channels,
            )
        };
        // Fails only once the env is closing, when nobody is listening anyway.
        self.tsfn
            .call(event, ThreadsafeFunctionCallMode::NonBlocking);
        Ok(())
    }
}

/// `Ok(None)` when the device doesn't exist or can't be activated.
pub fn on_endpoint_volume_change<'e>(
    env: &'e Env,
    callback: Function<Unknown<'static>, ()>,
    device_id: Option<&str>,
) -> Result<Option<Function<'e, (), ()>>> {
    let com = com_guard()?;
    let Some(device) = resolve_device(device_id)? else {
        return Ok(None);
    };
    let Some(endpoint) =
        activate::<IAudioEndpointVolume>(&device, "failed to activate endpoint volume")?
    else {
        return Ok(None);
    };
    let (tsfn, active) = gated_tsfn(env, "onEndpointVolumeChange", callback)?;
    let notifier: IAudioEndpointVolumeCallback = EndpointVolumeNotifier { tsfn }.into();
    unsafe { endpoint.RegisterControlChangeNotify(&notifier) }
        .map_err(|e| to_napi_err("failed to register endpoint volume notifications", e))?;
    subscribe(
        env,
        Subscription::new(com, move || unsafe {
            active.set(false);
            // Failing here only means the device is gone; dropping `notifier` releases the
            // threadsafe function either way.
            let _ = endpoint.UnregisterControlChangeNotify(&notifier);
        }),
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_event_context_is_self_initiated() {
        let ours = unsafe { *crate::com::event_context() };
        assert!(volume_event(&ours, false, 0.5, &[]).self_initiated);
        assert!(!volume_event(&GUID::zeroed(), false, 0.5, &[]).self_initiated);
    }

    #[test]
    fn maps_volume_mute_and_channels_in_order() {
        let event = volume_event(&GUID::zeroed(), true, 0.25, &[0.5, 1.0]);
        assert_eq!(event.volume, 0.25);
        assert!(event.muted);
        assert_eq!(event.channel_volumes, vec![0.5, 1.0]);
    }
}
