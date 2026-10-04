# PR 1: Module Split + Device Enumeration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `src/core_audio.rs` into `com.rs`/`sessions.rs`/`devices.rs` with no behaviour change, then add `listDevices`, `getDefaultDevice` and `getDevice` (pycaw `GetAllDevices`, `GetSpeakers`/`GetMicrophone`, `GetEndpointDataFlow`).

**Architecture:** `com.rs` owns COM plumbing (ComGuard, errors, enumerator, device resolution). `sessions.rs` keeps today's session code verbatim. `devices.rs` defines the napi `Device` object and string enums plus enumeration over `IMMDeviceEnumerator`. `lib.rs` stays the thin `#[napi]` function surface.

**Tech Stack:** Rust 2021, napi-rs 3 (`napi9`), `windows` 0.62, `node:test`, Bun.

**Spec:** `docs/superpowers/specs/2026-10-04-pycaw-parity-roadmap-design.md` (PR 1 + "Cross-cutting decisions").

## Context

The roadmap starts with PR 1. Every later PR needs `resolve_device(deviceId?)` and a module layout that isn't one big file. Users get a way to find device ids, which PRs 2–7 take as input.

## Global Constraints

- Flat functions, plain objects (`#[napi(object)]`), enums via `#[napi(string_enum)]` with exact lowercase/camelCase string values. No classes holding COM pointers.
- A missing device (`0x80070490`) gives `null`/`[]`, never a throw.
- Errors go through `to_napi_err(context, err)`.
- Reuse `ComGuard` (per-call MTA init). Fix its doc comment: sync napi calls run on the JS thread, not a fresh worker.
- Existing `listSessions`/`setProcessVolume`/`setProcessMute` behave identically.
- Default tests never mutate state and must pass on CI runners with **no audio devices**.
- Ship README section, one `examples/*.ts`, a `feat:` conventional commit, green `cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test`, `bun test`. No co-author trailers. No release.

## Review Focus

1. `getDevice('garbage')` / an unknown id → `null`, not a throw (HRESULT may be `E_NOTFOUND` or `E_INVALIDARG`). → Task 2 test.
2. `listDevices({ state: [] })` → `[]` (a zero mask must not reach `EnumAudioEndpoints`). → Task 2 test.
3. A device whose property store fails or has no name (common for `notPresent`) must not fail the whole list; `name` falls back to `DeviceDesc`, then `''`. → Task 2 test over all states.
4. Invalid enum strings (`flow: 'bogus'`) throw a clean napi error instead of crashing. → Task 2 test.
5. No-device machine: `getDefaultDevice()` → `null`, `listDevices()` → `[]`, `listSessions()` → `[]`. → Task 2 test (shape-agnostic).

## File Structure

- Create `src/com.rs` — `ComGuard`, `to_napi_err`, `validate_volume`, `ERROR_NOT_FOUND_HRESULT`, `device_enumerator()`, `resolve_device(Option<&str>)`, `default_device(flow, role)`.
- Create `src/sessions.rs` — everything session-related moved from `core_audio.rs`.
- Create `src/devices.rs` — napi enums/object, pure mapping helpers + unit tests, `list_devices`, `get_device`, `get_default_device`.
- Delete `src/core_audio.rs`.
- Modify `src/lib.rs`, `Cargo.toml`, `__test__/addon.test.mjs`, `README.md`, `AGENTS.md`.
- Create `examples/devices.ts`, `docs/superpowers/plans/2026-10-04-module-split-device-enumeration.md` (copy of this plan).

---

### Task 1: Module split (no behaviour change)

**Files:** Create `src/com.rs`, `src/sessions.rs`; delete `src/core_audio.rs`; modify `src/lib.rs`, `AGENTS.md`; add this plan to `docs/superpowers/plans/2026-10-04-module-split-device-enumeration.md`.

**Interfaces — Produces:**
- `com::ComGuard::new() -> windows::core::Result<ComGuard>`
- `com::to_napi_err(&str, windows::core::Error) -> napi::Error`
- `com::validate_volume(f64) -> napi::Result<()>`
- `com::device_enumerator() -> napi::Result<IMMDeviceEnumerator>`
- `com::default_device(EDataFlow, ERole) -> napi::Result<Option<IMMDevice>>`
- `com::resolve_device(Option<&str>) -> napi::Result<Option<IMMDevice>>`
- `sessions::{list_sessions, set_volume_for_process, set_mute_for_process, AudioSessionInfo}` (unchanged signatures)

- [ ] **Step 1: Baseline.** Run `pnpm build && pnpm test && cargo test`. Expected: all pass. This is the regression suite for the refactor.

- [ ] **Step 2: Create `src/com.rs`.** Move `validate_volume`, `to_napi_err` (now `pub`), `ComGuard` (now `pub`), `ERROR_NOT_FOUND_HRESULT`, and the `#[cfg(test)] mod tests` for `validate_volume` from `core_audio.rs`. Replace the ComGuard doc comment and add the enumerator/device helpers:

```rust
/// RAII guard: initializes COM (MTA) for the calling thread on construction and
/// uninitializes on drop. Synchronous napi calls run on the JS thread, so this pairs one
/// init/uninit per call on that thread.
pub struct ComGuard;

/// Returned when a device doesn't exist: no default endpoint (e.g. a headless CI runner) or
/// an unknown device id. A legitimate "nothing there" state, not a failure.
pub const ERROR_NOT_FOUND_HRESULT: i32 = 0x8007_0490_u32 as i32;

pub fn device_enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|e| to_napi_err("failed to create audio device enumerator", e))
}

/// Maps "device not found" to `Ok(None)`; every other failure becomes a napi error.
fn missing_as_none(
    result: windows::core::Result<IMMDevice>,
    context: &str,
) -> Result<Option<IMMDevice>> {
    match result {
        Ok(device) => Ok(Some(device)),
        Err(e) if e.code().0 == ERROR_NOT_FOUND_HRESULT => Ok(None),
        Err(e) => Err(to_napi_err(context, e)),
    }
}

pub fn default_device(flow: EDataFlow, role: ERole) -> Result<Option<IMMDevice>> {
    let enumerator = device_enumerator()?;
    missing_as_none(
        unsafe { enumerator.GetDefaultAudioEndpoint(flow, role) },
        "failed to get default audio endpoint",
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
            )
        }
    }
}
```

Imports: `napi::{Error, Result, Status}`, `windows::core::HSTRING`, `windows::Win32::Media::Audio::{eConsole, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator}`, `windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED}`.

- [ ] **Step 3: Create `src/sessions.rs`.** Move `AudioSessionInfo`, `process_name_for_pid`, `each_session`, `list_sessions`, `matching_sessions`, `set_volume_for_process`, `set_mute_for_process` verbatim. Replace `session_manager()` with a version built on `resolve_device`:

```rust
use crate::com::{resolve_device, to_napi_err, ComGuard};

/// Returns `Ok(None)` when there is no default render device, rather than an error.
fn session_manager() -> Result<Option<IAudioSessionManager2>> {
    let Some(device) = resolve_device(None)? else {
        return Ok(None);
    };
    unsafe { device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }
        .map(Some)
        .map_err(|e| to_napi_err("failed to activate audio session manager", e))
}
```

- [ ] **Step 4: Update `src/lib.rs`.** Replace `mod core_audio;` with `mod com; mod devices; mod sessions;` (create an empty `src/devices.rs` for now), and change call sites to `sessions::list_sessions()`, `com::validate_volume(volume)?`, `sessions::set_volume_for_process(...)`, `sessions::set_mute_for_process(...)`. Delete `src/core_audio.rs`.

- [ ] **Step 5: Verify no behaviour change.** Run `cargo fmt --check && cargo test && pnpm build && pnpm test && bun test`. Expected: identical pass results to Step 1 (3 Rust tests, same JS tests). If `JSCAW_TEST_PROCESS` is available locally, run `JSCAW_TEST_PROCESS=<app.exe> pnpm test` too.

- [ ] **Step 6: Update `AGENTS.md`** "Project Structure" sentence: `lib.rs` exposes the Node-API surface; `com.rs` holds COM init, error mapping and device resolution; `sessions.rs` holds audio-session enumeration and per-process volume/mute; `devices.rs` holds endpoint device enumeration.

- [ ] **Step 7: Save the plan into the repo and commit.**

```bash
git add -A src AGENTS.md docs/superpowers/plans/2026-10-04-module-split-device-enumeration.md
git commit -m "refactor: split core_audio into com, sessions and devices modules"
```

---

### Task 2: Device enumeration

**Files:** Modify `Cargo.toml`, `src/devices.rs`, `src/lib.rs`, `__test__/addon.test.mjs`, `README.md`; create `examples/devices.ts`.

**Interfaces:**
- Consumes: `com::{ComGuard, device_enumerator, default_device, resolve_device, to_napi_err}` from Task 1.
- Produces (TS, generated into `index.d.ts`):

```ts
export type DeviceFlow = 'render' | 'capture';
export type FlowFilter = 'render' | 'capture' | 'all';
export type DeviceState = 'active' | 'disabled' | 'notPresent' | 'unplugged';
export type DeviceRole = 'console' | 'multimedia' | 'communications';
export interface Device { id: string; name: string; flow: DeviceFlow; state: DeviceState }
export interface ListDevicesOptions { flow?: FlowFilter; state?: DeviceState[] }
export function listDevices(options?: ListDevicesOptions): Device[];
export function getDefaultDevice(flow?: DeviceFlow, role?: DeviceRole): Device | null;
export function getDevice(id: string): Device | null;
```

(napi-rs may emit string enums as `export declare enum`/union; either is fine as long as the string values match.)

- [ ] **Step 1: Write failing JS tests** — append to `__test__/addon.test.mjs` and extend the import with `listDevices, getDefaultDevice, getDevice`:

```js
const FLOWS = ['render', 'capture'];
const STATES = ['active', 'disabled', 'notPresent', 'unplugged'];
const assertDevice = (device) => {
  assert.equal(typeof device.id, 'string');
  assert.ok(device.id.length > 0);
  assert.equal(typeof device.name, 'string');
  assert.ok(FLOWS.includes(device.flow), `bad flow ${device.flow}`);
  assert.ok(STATES.includes(device.state), `bad state ${device.state}`);
};

test('listDevices defaults to active devices of both flows', () => {
  const devices = listDevices();
  assert.ok(Array.isArray(devices));
  for (const device of devices) {
    assertDevice(device);
    assert.equal(device.state, 'active');
  }
});

test('listDevices filters by flow', () => {
  for (const flow of FLOWS) {
    assert.ok(listDevices({ flow }).every((d) => d.flow === flow));
  }
  assert.ok(Array.isArray(listDevices({ flow: 'all' })));
});

test('listDevices returns every requested state without failing on nameless devices', () => {
  const devices = listDevices({ state: STATES });
  devices.forEach(assertDevice);
  assert.ok(devices.length >= listDevices().length);
});

test('listDevices with an empty state list returns []', () => {
  assert.deepEqual(listDevices({ state: [] }), []);
});

test('listDevices rejects unknown enum values', () => {
  assert.throws(() => listDevices({ flow: 'bogus' }));
  assert.throws(() => listDevices({ state: ['bogus'] }));
});

test('getDefaultDevice returns a matching device or null for every flow/role', () => {
  for (const flow of FLOWS) {
    for (const role of ['console', 'multimedia', 'communications']) {
      const device = getDefaultDevice(flow, role);
      if (device === null) continue; // no device of this flow (e.g. CI)
      assertDevice(device);
      assert.equal(device.flow, flow);
    }
  }
  const fallback = getDefaultDevice();
  if (fallback !== null) assert.equal(fallback.flow, 'render');
});

test('getDevice returns null for unknown ids', () => {
  assert.equal(getDevice('{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}'), null);
  assert.equal(getDevice('not-a-device-id'), null);
  assert.equal(getDevice(''), null);
});

test('getDevice round-trips ids from listDevices', () => {
  for (const device of listDevices({ state: STATES })) {
    assert.deepEqual(getDevice(device.id), device);
  }
});
```

- [ ] **Step 2: Run to confirm failure.** `pnpm build && pnpm test`. Expected: FAIL — `listDevices` is not exported.

- [ ] **Step 3: Add windows features** to `Cargo.toml` `[dependencies.windows] features`: `"Win32_Devices_FunctionDiscovery"`, `"Win32_UI_Shell_PropertiesSystem"` (keep the list alphabetical).

- [ ] **Step 4: Write failing Rust unit tests** for the pure helpers at the bottom of `src/devices.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_mask_ors_requested_states() {
        assert_eq!(state_mask(&[DeviceState::Active]), 1);
        assert_eq!(state_mask(&[DeviceState::Active, DeviceState::Unplugged]), 9);
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
            assert_eq!(DeviceState::from_windows(DEVICE_STATE(state_mask(&[state]))), Some(state));
        }
        assert_eq!(DeviceState::from_windows(DEVICE_STATE(0)), None);
    }

    #[test]
    fn flow_maps_both_ways() {
        assert_eq!(DeviceFlow::from_windows(eRender), Some(DeviceFlow::Render));
        assert_eq!(DeviceFlow::from_windows(eCapture), Some(DeviceFlow::Capture));
        assert_eq!(DeviceFlow::from_windows(eAll), None);
        assert_eq!(FlowFilter::All.to_windows(), eAll);
        assert_eq!(DeviceFlow::Capture.to_windows(), eCapture);
    }
}
```

Run `cargo test`. Expected: FAIL to compile (helpers undefined).

- [ ] **Step 5: Implement `src/devices.rs`.**

```rust
use crate::com::{default_device, device_enumerator, resolve_device, to_napi_err, ComGuard};
use napi::Result;
use napi_derive::napi;
use windows::core::Interface;
use windows::Win32::Devices::FunctionDiscovery::{PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName};
use windows::Win32::Media::Audio::{
    eAll, eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice,
    IMMEndpoint, DEVICE_STATE, DEVICE_STATEMASK_ALL, DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED,
    DEVICE_STATE_NOTPRESENT, DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::System::Com::{CoTaskMemFree, STGM_READ};

#[napi(string_enum)]
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceFlow {
    #[napi(value = "render")]
    Render,
    #[napi(value = "capture")]
    Capture,
}

#[napi(string_enum)]
#[derive(Debug, PartialEq, Eq)]
pub enum FlowFilter {
    #[napi(value = "render")]
    Render,
    #[napi(value = "capture")]
    Capture,
    #[napi(value = "all")]
    All,
}

#[napi(string_enum)]
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
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
    pub fn to_windows(self) -> EDataFlow {
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
        [Self::Active, Self::Disabled, Self::NotPresent, Self::Unplugged]
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
fn string_property(device: &IMMDevice, key: &windows::Win32::Foundation::PROPERTYKEY) -> String {
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
        CoTaskMemFree(Some(raw_id.0 as *const _));
        let flow = DeviceFlow::from_windows(device.cast::<IMMEndpoint>().ok()?.GetDataFlow().ok()?)?;
        let state = DeviceState::from_windows(device.GetState().ok()?)?;
        let mut name = string_property(device, &PKEY_Device_FriendlyName);
        if name.is_empty() {
            name = string_property(device, &PKEY_Device_DeviceDesc);
        }
        Some(Device { id, name, flow, state })
    }
}

fn com_guard() -> Result<ComGuard> {
    ComGuard::new().map_err(|e| to_napi_err("failed to initialize COM", e))
}

pub fn list_devices(options: Option<ListDevicesOptions>) -> Result<Vec<Device>> {
    let options = options.unwrap_or(ListDevicesOptions { flow: None, state: None });
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

pub fn get_default_device(flow: Option<DeviceFlow>, role: Option<DeviceRole>) -> Result<Option<Device>> {
    let _com = com_guard()?;
    let flow = flow.unwrap_or(DeviceFlow::Render).to_windows();
    let role = role.unwrap_or(DeviceRole::Console).to_windows();
    Ok(default_device(flow, role)?.as_ref().and_then(to_device))
}

pub fn get_device(id: String) -> Result<Option<Device>> {
    let _com = com_guard()?;
    Ok(resolve_device(Some(&id))?.as_ref().and_then(to_device))
}
```

Notes for the implementer:
- If napi-derive already derives `Clone`/`Copy` on string enums, the `self`-by-value methods compile as-is; if it doesn't, add `Clone, Copy` to the `#[derive]` lines. If it errors on a duplicate derive, remove it.
- `sessions.rs`'s `each_session` keeps its own ComGuard; reuse `com_guard()` there too only if it's a no-diff move (don't widen Task 2).

- [ ] **Step 6: Expose in `src/lib.rs`:**

```rust
use devices::{Device, DeviceFlow, DeviceRole, ListDevicesOptions};

#[napi]
pub fn list_devices(options: Option<ListDevicesOptions>) -> napi::Result<Vec<Device>> {
    devices::list_devices(options)
}

#[napi]
pub fn get_default_device(
    flow: Option<DeviceFlow>,
    role: Option<DeviceRole>,
) -> napi::Result<Option<Device>> {
    devices::get_default_device(flow, role)
}

#[napi]
pub fn get_device(id: String) -> napi::Result<Option<Device>> {
    devices::get_device(id)
}
```

- [ ] **Step 7: Run everything.** `cargo fmt && cargo test && pnpm build && pnpm test && bun test`. Expected: all pass.
  - If `getDevice('not-a-device-id')` or `getDevice('')` throws instead of returning `null`, read the HRESULT in the message and extend `missing_as_none` in `com.rs` to also treat that code (likely `E_INVALIDARG`, `0x80070057`) as missing **only on the `GetDevice` path** — pass the extra code in from `resolve_device` rather than widening the default-endpoint path.
  - Check `index.d.ts` shows the types from the Interfaces block.

- [ ] **Step 8: Example `examples/devices.ts`** (read-only, so it's safe to run anywhere):

```ts
import { getDefaultDevice, listDevices } from 'jscaw';

for (const device of listDevices({ state: ['active', 'unplugged'] })) {
  console.log(`${device.flow.padEnd(7)} ${device.state.padEnd(9)} ${device.name}  ${device.id}`);
}

console.log('default speakers:', getDefaultDevice('render')?.name ?? '(none)');
console.log('default microphone:', getDefaultDevice('capture')?.name ?? '(none)');
```

Run it locally against the build: `bun examples/devices.ts` won't resolve `'jscaw'` from the repo, so run `node -e "const j=require('./index.js'); console.log(j.listDevices(), j.getDefaultDevice('capture'))"` and confirm real devices print with sensible names.

- [ ] **Step 9: README.** Update the intro sentence to mention listing audio devices. Add a `## Devices` section after the first code block:

```md
## Devices

```ts
import { getDefaultDevice, getDevice, listDevices } from 'jscaw';

listDevices(); // active render + capture devices
// [{ id: '{0.0.0.00000000}.{…}', name: 'Speakers (Realtek(R) Audio)', flow: 'render', state: 'active' }, ...]
listDevices({ flow: 'capture', state: ['active', 'unplugged'] });
getDefaultDevice();                              // default speakers, or null
getDefaultDevice('capture', 'communications');   // default comms microphone, or null
getDevice(id);                                   // null when the id is unknown
```

`listDevices` defaults to `flow: 'all'` and `state: ['active']`. `name` is the device's
friendly name, falling back to its description. Machines with no audio devices get `[]`/`null`
rather than errors.
```

In "v1 exclusions", drop nothing yet except reword to "No audio routing (devices can be listed, not switched), no master/device volume control, …". Mention `examples/devices.ts` next to `examples/basic.ts`.

- [ ] **Step 10: Commit.**

```bash
git add Cargo.toml Cargo.lock src __test__/addon.test.mjs README.md examples/devices.ts
git commit -m "feat: add listDevices, getDefaultDevice and getDevice"
```

---

## Verification

- `cargo fmt --check`, `cargo test` (3 existing + 3 new unit tests), `pnpm build`, `pnpm test`, `bun test` — all green.
- Locally: the `node -e` check in Task 2 Step 8 lists real devices; `getDefaultDevice('capture')?.name` matches Windows Sound settings' default mic; disabling a device in Sound settings makes it disappear from `listDevices()` and appear in `listDevices({ state: ['disabled'] })`.
- `JSCAW_TEST_PROCESS=<playing app.exe> pnpm test` still passes (session refactor regression).
- Push branch, open PR (`feat:` title), let CI (x64 + arm64, Node 22/24, Bun — no audio devices) confirm the no-device paths.
