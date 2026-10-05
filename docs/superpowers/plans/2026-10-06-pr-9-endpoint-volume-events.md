# PR 9 — Endpoint volume notifications (pycaw `AudioEndpointVolumeCallback`)

## Context

Next PR on the pycaw parity roadmap (`docs/superpowers/specs/2026-10-04-pycaw-parity-roadmap-design.md`).
PRs 1–8 are merged; PR 9 depends only on PR 8 (event plumbing in `src/events/`). The goal: JS
can subscribe to master-volume/mute/channel changes on an endpoint, and tell its own changes
apart from external ones (`selfInitiated`) — the prerequisite for PR 10/11's "external change" logic.

First implementation step: save this plan as `docs/superpowers/plans/2026-10-06-pr-9-endpoint-volume-events.md`
(matching the PR 1–8 naming), then implement with TDD.

## API

```ts
onEndpointVolumeChange(
  callback: (e: { volume: number; muted: boolean; channelVolumes: number[]; selfInitiated: boolean }) => void,
  deviceId?: string,
): (() => void) | null
```
- `deviceId` omitted → default render/console (via existing `resolve_device`).
- Missing/unusable device → `null`, never a throw (roadmap cross-cutting rule; matches `getEndpointVolume`).
- Subscription keeps the event loop alive; `unsubscribe` is idempotent; env teardown unregisters.

## Changes

1. **`src/com.rs`** — process-wide event context:
   `pub fn event_context() -> *const GUID` backed by `static EVENT_CONTEXT: OnceLock<GUID>` filled with
   `GUID::new()` (random per process, like pycaw magic's `uuid4`; a const GUID would make other
   jscaw processes look "self").
2. **Setters pass it instead of `null()`** — `src/endpoint.rs` (`SetMasterVolumeLevelScalar`,
   `SetMasterVolumeLevel`, `SetMute`, `SetChannelVolumeLevelScalar`, `VolumeStepUp/Down`) and
   `src/sessions.rs` (`SetMasterVolume`, `SetMute`, `SetChannelVolume`, `SetDisplayName`,
   `SetIconPath`, `SetGroupingParam`) so PR 10 gets `selfInitiated` for free. Drop the
   now-unused `std::ptr::null` imports.
3. **`src/events/mod.rs`** — extract the gate+TSFN block from `events/device.rs:132-145` into
   `pub fn gated_tsfn<T: ToNapiValue + 'static>(env, name, callback) -> Result<(ThreadsafeFunction<T, …>, Rc<Cell<bool>>)>`;
   `device.rs` switches to it (no behaviour change). PR 10 reuses it three more times.
4. **`src/events/endpoint.rs`** (new):
   - `#[napi(object)] EndpointVolumeEvent { volume, muted, channel_volumes: Vec<f64>, self_initiated }`
   - pure `fn volume_event(ctx: &GUID, muted: bool, volume: f32, channels: &[f32]) -> EndpointVolumeEvent`
     (compares `ctx` against `event_context()`).
   - `#[implement(IAudioEndpointVolumeCallback)] EndpointVolumeNotifier { tsfn }`; `OnNotify` reads
     `AUDIO_VOLUME_NOTIFICATION_DATA` — channels via `slice::from_raw_parts(afChannelVolumes.as_ptr(), nChannels)`
     (flexible array member; SAFETY comment) — and emits NonBlocking.
   - `on_endpoint_volume_change(env, callback, device_id)`: `ComGuard`, `with_endpoint`-style resolve
     + `activate::<IAudioEndpointVolume>` (return `Ok(None)` when missing), `RegisterControlChangeNotify`,
     then `subscribe(env, Subscription::new(com, move || { active.set(false); let _ = ep.UnregisterControlChangeNotify(&cb); }))`.
5. **`src/lib.rs`** — `#[napi(strict, ts_args_type = "callback: (event: EndpointVolumeEvent) => void, deviceId?: string", ts_return_type = "(() => void) | null")]`
   wrapper; regenerate `index.d.ts`/`index.js` via `pnpm build`.
6. **Docs/examples** — README "Endpoint volume events" section after "Endpoint volume" (shrink
   "v1 exclusions" if it lists callbacks); `examples/endpoint-volume-events.ts`; AGENTS.md module
   layout mentions `events/endpoint.rs`.

Reused as-is: `Subscription`/`subscribe` (`events/mod.rs`), `resolve_device`/`activate`/`to_napi_err`/`ComGuard` (`com.rs`). No new Cargo features (`Win32_Media_Audio_Endpoints` already enabled).

## Tests (TDD)

- Rust unit (`events/endpoint.rs`): `volume_event` with our context → `self_initiated: true`; with a
  different/zero GUID → `false`; channel f32s map to f64 in order.
- `__test__/addon.test.mjs` (safe on CI, no audio):
  - unknown `deviceId` → `null`
  - non-function callback throws
  - with a default device: returns a function, idempotent unsubscribe (skip when `getDefaultDevice()` is null)
  - child-process: exits cleanly while subscribed; unsubscribe lets loop drain
- Gated live (`JSCAW_TEST_DEVICE`): subscribe, `setEndpointVolume(x)` → event with matching volume and
  `selfInitiated: true`; restore volume in `finally`.

## Verification

`cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test`, `bun test`; gated test with
`JSCAW_TEST_DEVICE`; manual: run the example, drag the Windows volume slider → events with
`selfInitiated: false`; `node -e` subscribed then `process.exit` exits cleanly.
Commit `feat: add endpoint volume notifications`, open PR.
