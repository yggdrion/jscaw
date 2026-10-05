# PR 7: Device Property Store Implementation Plan

> **For agentic workers:** execute inline with superpowers:executing-plans + superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** pycaw's `AudioDevice.properties` as `getDeviceProperties(deviceId) → Record<string, string | number | boolean | null> | null`.

**Spec:** `docs/superpowers/specs/2026-10-04-pycaw-parity-roadmap-design.md` (PR 7 + "Cross-cutting decisions").

## Context

PRs 1–6 have merged (`23cc44d` is main's tip). PR 7 depends only on PR 1. Today only `FriendlyName`/`DeviceDesc` are read (`string_property` in `src/devices.rs`), and the rest of the endpoint property store isn't exposed. The spec is approved, so this session goes straight to the plan and then TDD.

## Design

All of it goes in `src/devices.rs`, since it is device-scoped and already owns the property-store access. No new module.

- **Key format, matching pycaw:** `str(PROPERTYKEY)` gives `"%s %s" % (fmtid, pid)`, and comtypes GUIDs print as `{UPPER-HEX}`. In Rust that is `format!("{{{:?}}} {}", key.fmtid, key.pid)`, because windows' `GUID` Debug prints uppercase hyphenated with no braces. This goes in a pure helper, `property_key_name(&PROPERTYKEY) -> String`.
- **Value decoding:** a pure helper, `decode(&PROPVARIANT) -> Option<PropertyValue>`, matches on `value.Anonymous.Anonymous.vt` and reads the raw union `value.Anonymous.Anonymous.Anonymous`:
  - `VT_LPWSTR` → `pwszVal` as a String (an empty string when it is null)
  - `VT_BOOL` → `boolVal.as_bool()`
  - `VT_UI4` → `ulVal`, `VT_I4` → `lVal`, and `VT_UI8` → `uhVal as f64`
  - `VT_CLSID` → `*puuid` formatted `{GUID}`, with `None` when it is null
  - anything else → `None`, which becomes JS `null`
  - `// ponytail: VT_UI8 > 2^53 loses precision as a JS number; switch to BigInt if a real property needs it`
  - `PROPVARIANT` already implements `Drop` (`PropVariantClear`), so there is no manual cleanup.
- **Type:** `type PropertyValue = Either3<String, f64, bool>`. The function returns `Option<HashMap<String, Option<PropertyValue>>>`, so napi-rs emits `Record<string, string | number | boolean | null> | null`. Check the generated `index.d.ts`. If it prints worse, add `#[napi(ts_return_type = "...")]`.
- **Flow:**
  ```rust
  pub fn get_device_properties(id: String) -> Result<Option<HashMap<String, Option<PropertyValue>>>> {
      let _com = com_guard()?;
      let Some(device) = resolve_device(Some(&id))? else { return Ok(None) };
      let store = unsafe { device.OpenPropertyStore(STGM_READ) }
          .map_err(|e| to_napi_err("failed to open device property store", e))?;
      let count = unsafe { store.GetCount() }.map_err(|e| to_napi_err("failed to get property count", e))?;
      let mut props = HashMap::new();
      for i in 0..count {
          let mut key = PROPERTYKEY::default();
          // Like pycaw, a key or value that fails to read is skipped.
          if unsafe { store.GetAt(i, &mut key) }.is_err() { continue; }
          let Ok(value) = (unsafe { store.GetValue(&key) }) else { continue };
          props.insert(property_key_name(&key), decode(&value));
      }
      Ok(Some(props))
  }
  ```
  An unknown or malformed id gives `null`, matching `getDevice`. The store and device are locals, so they drop before `_com`.
- **`src/lib.rs`:** one thin `#[napi]` wrapper, `get_device_properties(device_id: String)`.
- **Reuse:** `com_guard`, `resolve_device`, `to_napi_err` and `STGM_READ` are all already imported in `devices.rs`. `Cargo.toml` already has `Win32_System_Variant`, `Win32_System_Com_StructuredStorage` and `Win32_UI_Shell_PropertiesSystem`, so it doesn't change.

Skipped: other VT types like blobs and vectors (null per the spec), and refactoring `string_property` onto `decode` (it works, and it uses `Display`'s broader coercion).

## Tasks

### Task 1: Rust helpers (TDD, `#[cfg(test)]` in `src/devices.rs`)

- [ ] Failing tests first:
  - `property_key_name(&PKEY_Device_FriendlyName) == "{A45C254E-DF1C-4EFD-8020-67D146A850E0} 14"`
  - `decode` on PROPVARIANTs built with the existing `From` impls (`From<&str>`, `From<bool>`, `From<u32>`, `From<i32>`, `From<u64>`) gives String, bool and numbers. Check that the `bool` conversion produces `VT_BOOL`. If `From<GUID>` is missing, build a `VT_CLSID` raw value by hand. `PROPVARIANT::default()` (`VT_EMPTY`) gives `None`.
- [ ] Implement both helpers. `cargo test` should be green.

### Task 2: napi surface + JS tests

**Files:** `src/devices.rs`, `src/lib.rs`, `__test__/addon.test.mjs`

- [ ] Default tests (non-mutating), with `getDeviceProperties` added to the imports:
  - It returns `null` for `BAD_ID`, `''` and the zero-GUID id already used by the peak tests.
  - For each `listDevices()` device (zero on CI): the result is an object, every key matches `/^\{[0-9A-F-]{36}\} \d+$/`, and every value is a string, number, boolean or null. When the friendly-name key is present, it equals `device.name`.
- [ ] `pnpm build` and `pnpm test` should be green, and `index.d.ts` should show the expected signature.

### Task 3: Docs + example + commit

- [ ] `examples/device-properties.ts`: list active devices and print each one's property table, noting the friendly-name key.
- [ ] README: add a "Device properties" section that explains the key format and the decoded types, and drop the related item from "v1 exclusions" if one is listed there.
- [ ] `cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test`.
- [ ] Commit with `feat: add device property store` (semantic-commits skill), then go through superpowers:finishing-a-development-branch to the PR.

## Verification

- `cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test`
- Locally, with audio: `node --experimental-strip-types examples/device-properties.ts` (or the runner the other examples use). Check that the friendly name, the `{GUID}`-valued keys and the booleans decode, and that unknown types print `null`.
- No live-mutation test is needed, since this is read-only.
