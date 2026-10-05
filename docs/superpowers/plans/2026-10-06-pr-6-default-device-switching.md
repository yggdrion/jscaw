# PR 6: Default-Device Switching Implementation Plan

> **For agentic workers:** execute inline with superpowers:executing-plans + superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**First execution step:** copy this file to `docs/superpowers/plans/2026-10-06-pr-6-default-device-switching.md` (repo convention) and include it in the commit.

**Goal:** Implement pycaw's `SetDefaultDevice` as `setDefaultDevice(deviceId, roles = ['console','multimedia','communications']) → boolean`, using the undocumented `IPolicyConfig`.

**Spec:** `docs/superpowers/specs/2026-10-04-pycaw-parity-roadmap-design.md` (PR 6 + "Cross-cutting decisions").

## Context

PRs 1–5 have merged (`e965e74` is the tip of main). PR 6 depends only on PR 1. Today devices can be listed but not switched: the README's "v1 exclusions" says "No audio routing". The spec is approved, so this session goes straight to the plan and then TDD.

## Design

- **New `src/policy_config.rs`** (this is the spec's module name):
  - The interface is declared with `#[windows::core::interface("f8679f50-850a-41cf-9c72-430f290290c8")] unsafe trait IPolicyConfig: IUnknown`. The macro expands to `::windows_core` paths, so `Cargo.toml` gains a direct `windows-core = "0.62"` dependency. It is already in the lockfile through `windows`, so nothing new is downloaded.
  - The vtable order must match exactly: `GetMixFormat, GetDeviceFormat, ResetDeviceFormat, SetDeviceFormat, GetProcessingPeriod, SetProcessingPeriod, GetShareMode, SetShareMode, GetPropertyValue, SetPropertyValue, SetDefaultEndpoint(PCWSTR, ERole) -> HRESULT, SetEndpointVisibility`.
  - Only `SetDefaultEndpoint` gets a real signature. The other 10 slots are `unsafe fn _slotN(&self) -> HRESULT;` placeholders that are never called. A comment explains that they only reserve vtable positions.
  - `const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);`
  - **Vista fallback skipped (user decision):** `IPolicyConfigVista` only matters on Vista/7, and Node and Bun need Win10+. The plan and the README note this deviation from the spec.
  - Flow:
    ```rust
    pub fn set_default_device(device_id: String, roles: Option<Vec<DeviceRole>>) -> Result<bool> {
        let roles = roles.unwrap_or_else(|| vec![Console, Multimedia, Communications]);
        // empty roles → Err(InvalidArg "roles must not be empty")
        let _com = com_guard()?;
        if resolve_device(Some(&device_id))?.is_none() { return Ok(false); } // matches endpoint setters
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
            .map_err(|e| to_napi_err("failed to create policy config", e))?;
        for role in roles { policy.SetDefaultEndpoint(&HSTRING::from(&*device_id), role.to_windows()).ok()
            .map_err(|e| to_napi_err("failed to set default audio endpoint", e))?; }
        Ok(true)
    }
    ```
    - Bind the device and policy to locals so they drop before `_com`, like `get_default_device`.
    - Reuse `com_guard`, `DeviceRole::to_windows` (make both `pub(crate)` in `src/devices.rs`), `resolve_device`, `to_napi_err` and `ComGuard`.
- **Return type `boolean`, not `void`:**
  - An unknown or malformed id gives `false` with no throw, which matches `setEndpointVolume(…, BAD_ID) === false`.
  - A non-zero HRESULT from `SetDefaultEndpoint` throws, as the spec requires. An example is a disabled device that does resolve.
- **`src/lib.rs`:** one thin `#[napi]` wrapper, `set_default_device(device_id: String, roles: Option<Vec<DeviceRole>>)`, and `mod policy_config;`.
- **MTA risk:** `CPolicyConfigClient` is normally used from an STA. Assume it works under the existing MTA `ComGuard`, and verify that in the gated live test and the manual run. If it fails with `REGDB_E_CLASSNOTREG`/`E_NOINTERFACE`, stop and re-plan.

Skipped: other `IPolicyConfig` methods (out of scope per the spec), dedupe of repeated roles (harmless), and Rust unit tests (no new pure helper; the role mapping is already tested).

## Tasks

### Task 1: Native + tests (TDD)

**Files:** `src/policy_config.rs` (new), `src/devices.rs`, `src/lib.rs`, `__test__/addon.test.mjs`

- [ ] Add `setDefaultDevice` to the test imports. Add these default (non-mutating) tests:
  - `setDefaultDevice returns false for unknown ids`: `BAD_ID`, `''` and the zero-GUID id from the peak tests.
  - `setDefaultDevice rejects bad roles`: `[]` throws `/roles must not be empty/`, and `['sideways']` throws.
- [ ] Add a gated live test (`JSCAW_TEST_DEVICE`; it skips unless that device is an active one):
  - Record `getDefaultDevice(flow, role)` for all 3 roles, where `flow` is the target's flow.
  - Call `setDefaultDevice(id)` and expect it to return `true`. All 3 roles should then report `id`.
  - Call `setDefaultDevice(id, ['communications'])` again to cover the explicit-roles path.
  - In `finally`, restore each role's previous default with `setDefaultDevice(prev.id, [role])`.
- [ ] `pnpm build:debug && pnpm test`. The new tests should fail (the export is missing).
- [ ] Implement the design above.
- [ ] `pnpm build:debug && pnpm test` should pass. Also run `cargo fmt` and `cargo test`.

### Task 2: Docs + example

- [ ] `README.md`:
  - Add a `## Default device` section after Devices. Show the import and the call with and without roles, note the `false` vs throw semantics, and say it **relies on the undocumented `IPolicyConfig` Windows API (Win10+; no Vista fallback)**.
  - Remove "No audio routing (devices can be listed, not switched)" from v1 exclusions, and update the top description.
  - Add `examples/default-device.ts` to the examples list.
- [ ] `examples/default-device.ts`:
  - Lists render devices, marking the console default.
  - With `process.argv[2]` set to a device id, it calls `setDefaultDevice` and prints the new default. Otherwise it is read-only.
- [ ] `AGENTS.md`: add `policy_config.rs` (default-device switching via undocumented `IPolicyConfig`) to the module-layout sentence.

### Task 3: Verify + commit + PR

- [ ] `cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test` and `bun test` should all be green (superpowers:verification-before-completion).
- [ ] Run the gated test locally with `JSCAW_TEST_DEVICE=<another active render id>`. Confirm the defaults are restored afterwards.
- [ ] Manual: `node --experimental-strip-types examples/default-device.ts <id>`. The Windows sound flyout should switch. Switch back afterwards.
- [ ] Commit `feat: add default device switching`, with no co-author trailers. Push and open the PR (body: behavior, undocumented-API note, win32 x64/arm64, commands run). Bind it with the ccd_pr tools.

## Verification

Covered in Task 3. The key review points are:
- The vtable slot order is exactly 10 slots before `SetDefaultEndpoint`.
- An unknown id gives `false` with no throw.
- An empty roles array throws.
- The live test restores every role's default.
