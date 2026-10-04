# jscaw → pycaw parity roadmap

## Context

jscaw (Rust + napi-rs, Windows-only) currently exposes three functions: `listSessions()`, `setProcessVolume(name, v)` and `setProcessMute(name, m)`. They cover only the default render endpoint, and sessions report only pid, process name, volume and mute. The README lists routing, master volume, meters and callbacks as "v1 exclusions".

The goal is **full parity with pycaw** (AndreMiras/pycaw, `main` branch). That covers devices, endpoint volume, rich sessions, meters, default-device switching, every notification interface, and the `magic.py` high-level layer. The work is split into small single-scope PRs, each runnable in its own session.

**Out of scope:** `IAudioClient` streaming (pycaw defines it but never uses it) and `IPolicyConfigVista` format/share-mode getters and setters (pycaw only uses its `SetDefaultEndpoint` fallback).

## How to use this roadmap

Each PR below is self-contained and meant to run in its own session. Start a session with
**"implement PR N from `docs/superpowers/specs/2026-10-04-pycaw-parity-roadmap-design.md`"**.
That session reads this spec and the current code (earlier PRs may have changed it), writes
`docs/superpowers/plans/<date>-<pr-slug>.md` with `superpowers:writing-plans`, then
implements with TDD. Respect the dependency graph: do not start a PR until its prerequisites
have merged.

## Cross-cutting decisions (apply to every PR)

- **API style:** flat functions that take string ids and return plain objects (`#[napi(object)]`). Enums use `#[napi(string_enum)]`, so the TS sees `'render' | 'capture'` and not numbers. No classes hold live COM pointers.
- **Device targeting:** every device-scoped function takes an optional `deviceId?: string`. When it's omitted, the function uses the default `render`/`console` endpoint, matching current behaviour.
- **Session targeting:** `SessionTarget = { pid } | { processName } | { instanceId }`, plus an optional `deviceId`.
  - `processName` matches case-insensitively and applies to every matching session.
  - Setters return the number of sessions changed, like today.
- **Backward compatibility:** the existing three functions keep working. `listSessions()` only gains fields.
- **No default device:** a missing device (`0x80070490`) gives `null` or `[]`, never a throw. This reuses the existing `ERROR_NOT_FOUND_HRESULT` handling in `src/core_audio.rs`.
- **Errors:** go through the existing `to_napi_err(context, err)`. Validation follows the existing pattern, with `validate_volume` reused for every 0..1 scalar.
- **COM:** reuse `ComGuard` (per-call MTA init). Note that the doc comment saying calls run "on a fresh worker thread" is wrong: sync napi calls run on the JS thread. PR 1 fixes the comment, and PR 8 depends on getting this right.
- **Module layout:** PR 1 splits `src/core_audio.rs` into `src/com.rs` (ComGuard, errors, `device_enumerator()`, `resolve_device(deviceId?)`), `src/devices.rs` and `src/sessions.rs`. Later PRs add `endpoint.rs`, `meter.rs`, `policy_config.rs` and `events/`. `lib.rs` stays the thin napi surface.
- **Tests:** `node:test` in `__test__/addon.test.mjs`.
  - Default tests never mutate real state. They check shapes, invalid inputs, and that unknown ids give clean errors or nulls. CI runners have no audio devices, so code paths must tolerate that.
  - Live mutation tests are gated by env vars: `JSCAW_TEST_PROCESS` (exists) and a new `JSCAW_TEST_DEVICE`. They always restore state in `finally`.
  - Pure Rust helpers get `#[cfg(test)]` unit tests: validators, PROPVARIANT decoding, enum mapping.
- **Each PR also ships** README sections (shrinking "v1 exclusions"), one `examples/*.ts`, a `feat:` conventional commit, and a green `cargo fmt --check`, `pnpm build` and `pnpm test`. Releases stay your call (`pnpm release minor`).

## PR breakdown

Dependency graph: `1 → {2, 3, 5, 6, 7, 8}`, `3 → {4, 10}`, `8 → {9, 10}`, `10 → 11`.

### PR 1: Module split + device enumeration (pycaw `GetAllDevices`, `GetSpeakers`/`GetMicrophone`, `GetEndpointDataFlow`)

- Refactor `core_audio.rs` into the module layout above. This is a no-behaviour-change step.
- Add `Device { id, name, flow: 'render'|'capture', state: 'active'|'disabled'|'notPresent'|'unplugged' }`. `name` comes from `PKEY_Device_FriendlyName`, falling back to `DeviceDesc`.
- `listDevices({ flow?: 'render'|'capture'|'all', state?: DeviceState[] }) → Device[]`. Defaults: `all` flows, `active` state.
- `getDefaultDevice(flow = 'render', role: 'console'|'multimedia'|'communications' = 'console') → Device | null`
- `getDevice(id) → Device | null`
- `Cargo.toml` gains the windows features for the property store (`Win32_UI_Shell_PropertiesSystem`, `Win32_Devices_FunctionDiscovery` for PKEYs).

### PR 2: Endpoint (master) volume (pycaw `IAudioEndpointVolume`, `volume_percent`)

- `getEndpointVolume(deviceId?) → { volume, volumeDb, muted, channels: { volume, volumeDb }[], range: { minDb, maxDb, incrementDb }, step: { current, count }, hardwareSupport: number } | null`
- Setters:
  - `setEndpointVolume(v, deviceId?)`
  - `setEndpointVolumeDb(db, deviceId?)`, which validates against the range
  - `setEndpointMute(m, deviceId?)`
  - `setEndpointChannelVolume(channel, v, deviceId?)`
  - `stepEndpointVolume('up'|'down', deviceId?)`
- Works for capture devices too (microphone volume).

### PR 3: Rich sessions + session targeting (pycaw `AudioSession`, `GetProcessSession`, `IAudioSessionControl2`)

- `listSessions({ deviceId?, includeSystemSounds? = false })`. Every session gains:
  - `state: 'inactive'|'active'|'expired'`
  - `displayName`, `iconPath`, `groupingParam`
  - `sessionId`, `instanceId`
  - `isSystemSounds`
  The `pid: 0` system-sounds session is listed only when opted in.
- Setters (all `(target, value) → count`):
  - `setSessionVolume`, `setSessionMute`
  - `setSessionDisplayName`, `setSessionIconPath`, `setSessionGroupingParam`
  - `setSessionDuckingPreference(target, optOut)`
- `setProcessVolume`/`setProcessMute` become thin wrappers over `{ processName }`.
- Generalise `each_session` to take a device and hand the visitor the `IAudioSessionControl2`. Keep one enumeration path, as AGENTS.md requires.

### PR 4: Per-session channel volume (pycaw `IChannelAudioVolume`)

- `getSessionChannelVolumes(target) → number[][]`, one array per matched session
- `setSessionChannelVolume(target, channel, v) → count`

### PR 5: Peak meters (pycaw `IAudioMeterInformation`)

- `getEndpointPeak(deviceId?) → number | null`
- `getSessionPeak(target) → number[]`, via a cast of the session control

### PR 6: Default-device switching (pycaw `SetDefaultDevice`, `IPolicyConfig`/`IPolicyConfigVista`)

- Define the undocumented interfaces with `#[windows::core::interface("f8679f50-…")]` plus the Vista IID fallback, and CLSID `870af99c-…`.
- `setDefaultDevice(deviceId, roles: Role[] = ['console', 'multimedia', 'communications'])`. It throws on a non-zero HRESULT.
- The README marks it as relying on an undocumented Windows API.

### PR 7: Device property store (pycaw `AudioDevice.properties`, `IPropertyStore`, `PROPVARIANT`)

- `getDeviceProperties(deviceId) → Record<string, string | number | boolean | null>`, with keys formatted `"{FMTID} pid"` exactly like pycaw.
- Decode `VT_LPWSTR`, `VT_BOOL`, `VT_UI4`/`VT_I4`, `VT_UI8` and `VT_CLSID`. Other types become `null`. A property that fails to read is skipped, as in pycaw.

### PR 8: Event infrastructure + device notifications (pycaw `MMNotificationClient`)

- Shared plumbing in `src/events/`:
  - an `#[implement]` COM object, a `ThreadsafeFunction` (NonBlocking) and a registry of live subscriptions
  - unsubscribe calls `Unregister…` and drops the TSFN
  - a napi env cleanup hook unregisters everything, so Node or Bun exiting doesn't crash
  - subscriptions keep the event loop alive, like `fs.watch`
- Handle COM apartment init on the JS thread, including `RPC_E_CHANGED_MODE`. The registered COM object and the enumerator are kept alive in the registry.
- `onDeviceEvent(cb) → unsubscribe`, where `DeviceEvent` is one of:
  - `{ type: 'added'|'removed', deviceId }`
  - `{ type: 'stateChanged', deviceId, state }`
  - `{ type: 'defaultChanged', flow, role, deviceId | null }`
  - `{ type: 'propertyChanged', deviceId, key: "{FMTID} pid" }`
- Tests: subscribe then unsubscribe without crashing, an unsubscribe that's idempotent, and process exit while still subscribed.

### PR 9: Endpoint volume notifications (pycaw `AudioEndpointVolumeCallback`)

- `onEndpointVolumeChange(cb, deviceId?) → unsubscribe`, where `cb({ volume, muted, channelVolumes, selfInitiated })`.
- Add a process-wide jscaw event-context GUID. Every setter from PRs 2–4 passes it instead of `null`, and `selfInitiated` compares against it. This is pycaw magic's GUID trick.

### PR 10: Session notifications (pycaw `AudioSessionNotification`, `AudioSessionEvents`, `IAudioVolumeDuckNotification`)

- `onSessionCreated(cb, deviceId?) → unsubscribe`. It calls `GetSessionEnumerator()` after registering, which is the documented activation gotcha.
- `onSessionEvent(target, cb) → unsubscribe`. Events:
  - `displayNameChanged`, `iconPathChanged`
  - `volumeChanged { volume, muted, selfInitiated }`
  - `channelVolumeChanged`, `groupingChanged`
  - `stateChanged { state }`
  - `disconnected { reason: 'deviceRemoval'|'serverShutdown'|'formatChanged'|'sessionLogoff'|'sessionDisconnected'|'exclusiveModeOverride' }`
- `onDuckEvent(cb, deviceId?) → unsubscribe`, with `{ type: 'duck'|'unduck', sessionId, activeSessionCount? }`

### PR 11: `jscaw/magic` high-level layer (pycaw `magic.py`)

- Pure hand-written JS + `.d.ts`, added to `files` and exposed through a package.json `exports` subpath. No new native code.
- `watchApp(exeNames, { onVolume, onMute, onState, onSessionsChanged, includeSelf? }) → MagicApp`. It tracks matching sessions as they are created or expire, using PRs 3 and 10.
  - `volume` and `mute` getters/setters fan out to every session. Volume reads give the max, and muted wins.
  - `toggleMute()`, `stepVolume(step = 0.1)`, `dispose()`
  - Callbacks fire only for external changes unless `includeSelf` is set.
- Skip pycaw's `MagicSession` subclassing; `onSessionEvent` already covers it.

## Verification (per PR, run in its own session)

- `cargo fmt --check`, `cargo test` for the Rust unit tests, `pnpm build`, `pnpm test`, `bun test`
- Locally on a machine with audio: run the PR's example. Run the gated live tests with `JSCAW_TEST_DEVICE`/`JSCAW_TEST_PROCESS` and confirm state is restored.
- Event PRs: a manual check that plugs or unplugs a device, or changes volume in the Windows mixer, and sees the callbacks fire. Then confirm `node -e` exits cleanly while subscribed.
- Before the release that ships the PR: `pnpm smoke-test`.

## Critical files

`src/lib.rs`, `src/core_audio.rs` (split in PR 1), `Cargo.toml` (windows features), `__test__/addon.test.mjs`, `README.md`, `examples/`, `package.json` (PR 11 `exports`/`files`), `AGENTS.md` (update the module layout description in PR 1).
