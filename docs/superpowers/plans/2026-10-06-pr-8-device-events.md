# PR 8: Event Infrastructure + Device Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development) + superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax. First step of execution: copy this file to `docs/superpowers/plans/2026-10-06-pr-8-device-events.md` and commit it with the code.

**Goal:** pycaw's `MMNotificationClient` as `onDeviceEvent(cb) → unsubscribe`, built on reusable event plumbing that PRs 9 and 10 can share.

**Architecture:** An `#[implement(IMMNotificationClient)]` COM object forwards each callback through a NonBlocking napi `ThreadsafeFunction` to JS. A per-JS-thread registry owns every live subscription: the COM init guard, the enumerator, the registered client and the TSFN. Removing a subscription unregisters it in `Drop`, and a napi env cleanup hook clears the registry on exit.

**Tech Stack:** Rust, napi 3.14 / napi-derive 3.6, windows 0.62 (`#[implement]`), `node:test`.

**Spec:** `docs/superpowers/specs/2026-10-04-pycaw-parity-roadmap-design.md`. Read PR 8 and "Cross-cutting decisions".

## Context

PRs 1–7 have merged (`efb3290` is main's tip), and PR 8 depends only on PR 1. No event API exists yet. PRs 9 (endpoint volume callback) and 10 (session callbacks) reuse this PR's `src/events/` plumbing, so its registry and subscription shape is what those PRs will build on.

## Global Constraints

- Flat functions, plain objects, and `#[napi(string_enum)]` enums. No JS classes hold COM pointers.
- Errors go through `to_napi_err`. A missing device is never a throw.
- CI has no audio devices, so default tests must not need any. Live tests are gated by `JSCAW_TEST_DEVICE` and restore state in `finally`.
- Subscriptions keep the event loop alive, like `fs.watch`. Exiting Node or Bun while subscribed must not crash.
- Ships with: a README section (remove "callbacks" from the v1 exclusions), `examples/device-events.ts`, an AGENTS.md module-layout line, a `feat:` commit, and green `cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test` and `bun test`.

## Design

### COM apartment (`src/com.rs`)
`ComGuard` becomes `pub struct ComGuard { owns_init: bool }`. `CoInitializeEx(MTA)` returning `RPC_E_CHANGED_MODE` (the thread is already STA, as in Electron) gives `Ok(ComGuard { owns_init: false })`, and `Drop` only calls `CoUninitialize` when `owns_init`. This also fixes every existing function on STA hosts, which today fail outright. Inits are refcounted per thread, so each subscription holds its own guard, and COM stays initialized on the JS thread while any subscription lives.

### Event types (`src/events/device.rs`)
```rust
#[napi(string_enum)] pub enum DeviceEventType { #[napi(value="added")] Added, #[napi(value="removed")] Removed,
  #[napi(value="stateChanged")] StateChanged, #[napi(value="defaultChanged")] DefaultChanged,
  #[napi(value="propertyChanged")] PropertyChanged }

#[napi(object)]
pub struct DeviceEvent {
    pub r#type: DeviceEventType,     // js: type
    pub device_id: Option<String>,   // null only for defaultChanged with no default
    pub state: Option<DeviceState>,  // stateChanged
    pub flow: Option<DeviceFlow>,    // defaultChanged
    pub role: Option<DeviceRole>,    // defaultChanged
    pub key: Option<String>,         // propertyChanged, "{FMTID} pid"
}
```
Add `#[napi(ts_type = ...)]` (or a `ts_args_type` on `onDeviceEvent`) so `index.d.ts` gets the spec's discriminated union:
`{type:'added'|'removed',deviceId:string} | {type:'stateChanged',deviceId:string,state:DeviceState} | {type:'defaultChanged',flow:DeviceFlow,role:DeviceRole,deviceId:string|null} | {type:'propertyChanged',deviceId:string,key:string}`.
Only the fields a variant needs are set. The rest stay `None`, and napi omits them. `ponytail:` comment: napi-rs 3 structured enums (`#[napi(discriminant = "type")]`) would generate the union directly. Try that first in Step 2. If it emits the right TS with camelCase fields, use it and delete the struct and `ts_type`.

Reuse from `src/devices.rs`, flipping each helper to `pub(crate)`: `DeviceState::from_windows`, `DeviceFlow::from_windows`, `property_key_name`, `com_guard`. Add `DeviceRole::from_windows(ERole) -> Option<Self>` next to `to_windows`.

### Pure mapping helpers (unit-testable, no COM)
```rust
fn added(id: String) -> DeviceEvent
fn removed(id: String) -> DeviceEvent
fn state_changed(id: String, state: DEVICE_STATE) -> Option<DeviceEvent>      // unknown state → None (dropped)
fn default_changed(flow: EDataFlow, role: ERole, id: Option<String>) -> Option<DeviceEvent>  // eAll/unknown → None
fn property_changed(id: String, key: &PROPERTYKEY) -> DeviceEvent
```

### COM client
```rust
#[implement(IMMNotificationClient)]
struct DeviceNotifier { tsfn: DeviceTsfn }
// DeviceTsfn = ThreadsafeFunction<DeviceEvent, (), DeviceEvent, Status, false>
impl IMMNotificationClient_Impl for DeviceNotifier_Impl {
    // each: build the event (PCWSTR → String via `.to_string()`, a null default id → None),
    // then `self.tsfn.call(ev, ThreadsafeFunctionCallMode::NonBlocking)`, ignoring the status
    // (a closing TSFN during shutdown is expected). Always return Ok(()).
}
```
Callbacks arrive on an MMDevAPI worker thread. The `#[implement]` object is agile and the TSFN is `Send + Sync`, so no marshalling is needed. The callback never calls back into COM.

### Registry (`src/events/mod.rs`, shared by PRs 9–10)
```rust
pub struct Subscription { unregister: Box<dyn FnOnce()>, _com: ComGuard }  // field order: unregister runs in Drop before _com drops
impl Drop for Subscription { fn drop(&mut self) { take(self.unregister)() } }

thread_local! { static REGISTRY: RefCell<Registry> }   // per JS thread → Worker-safe
struct Registry { next_id: u32, subs: HashMap<u32, Subscription>, cleanup_hooked: bool }

pub fn subscribe(env: &Env, sub: Subscription) -> Result<Function<'_, (), ()>>
   // insert; on first use per env, env.add_env_cleanup_hook(.., |_| REGISTRY.with(|r| r.borrow_mut().subs.clear()));
   // return env.create_function_from_closure("unsubscribe", move |_| { remove(id); Ok(()) })   // remove of a missing id = no-op → idempotent
```
The `unregister` closure for device events captures `enumerator: IMMDeviceEnumerator` and `client: IMMNotificationClient`, and calls `UnregisterEndpointNotificationCallback(&client)`, ignoring the error. Dropping the closure then drops the client, which drops the TSFN, which releases the event-loop ref. Unsubscribing therefore lets the process exit.

If the exact napi 3 names differ (`build_threadsafe_function`, `create_function_from_closure`, `add_env_cleanup_hook`), check them on docs.rs for napi 3.14 and adapt. The shape stays the same.

### Entry point
```rust
// src/events/device.rs
pub fn on_device_event<'e>(env: &'e Env, cb: Function<DeviceEvent, ()>) -> Result<Function<'e, (), ()>> {
    let com = com_guard()?;
    let enumerator = device_enumerator()?;
    let tsfn = cb.build_threadsafe_function().callee_handled::<false>().build()?;
    let client: IMMNotificationClient = DeviceNotifier { tsfn }.into();
    unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }
        .map_err(|e| to_napi_err("failed to register device notifications", e))?;
    events::subscribe(env, Subscription::new(com, move || unsafe { let _ = enumerator.UnregisterEndpointNotificationCallback(&client); }))
}
// src/lib.rs
#[napi(ts_return_type = "() => void")]
pub fn on_device_event(env: Env, callback: Function<DeviceEvent, ()>) -> napi::Result<Function<(), ()>>
```

`Cargo.toml`: add the `implement` feature to `windows` if `#[implement]` needs it in 0.62, plus `napi` feature `"napi4"`/tsfn if it isn't implied by `napi9` (it is, since napi9 ⊇ napi4).

## Review Focus

1. **Process exit while subscribed** (`process.exit()` or a signal): the child must exit 0 and never crash. Covered by a child-process test in Task 3.
2. **Unsubscribe called twice, or after another unsubscribe:** a no-op with no throw. Covered in Task 3.
3. **Subscription keeps the loop alive, and unsubscribe releases it:** a child that subscribes then unsubscribes exits on its own, and one that only subscribes is still alive after 1s. Covered in Task 3.
4. **A non-function callback:** throws a TypeError-like napi error, with nothing registered. Covered in Task 3.
5. **The JS callback throws:** with `callee_handled::<false>`, napi surfaces it as an uncaught exception, which matches EventEmitter semantics. Documented in the README, not tested.

---

### Task 1: `ComGuard` tolerates STA threads

**Files:** Modify `src/com.rs` (ComGuard + tests).

- [ ] Failing unit test in `com.rs`: spawn a `std::thread`, call `CoInitializeEx(None, COINIT_APARTMENTTHREADED)`, then `ComGuard::new()` must be `Ok` with `owns_init == false`. Drop it, then `CoUninitialize()`. A second test on a fresh thread checks that `ComGuard::new()` gives `owns_init == true`.
- [ ] `cargo test`. It fails, because today it returns the `RPC_E_CHANGED_MODE` error.
- [ ] Implement `owns_init`, match `RPC_E_CHANGED_MODE`, and update the doc comment.
- [ ] `cargo test` should be green, and `pnpm build && pnpm test` should still be green.

### Task 2: Event types + pure mapping helpers

**Files:** Create `src/events/mod.rs` (just `pub mod device;` for now) and `src/events/device.rs`. Modify `src/devices.rs` (make helpers `pub(crate)`, add `DeviceRole::from_windows`) and `src/lib.rs` (`mod events;`).

**Produces:** `DeviceEvent`, `DeviceEventType`, `added`/`removed`/`state_changed`/`default_changed`/`property_changed`.

- [ ] Failing `#[cfg(test)]` tests in `device.rs`:
  - `state_changed("x", DEVICE_STATE_UNPLUGGED)` gives type `StateChanged`, `state == Some(Unplugged)`, and `device_id == Some("x")`
  - `state_changed("x", DEVICE_STATE(0x99))` gives `None`
  - `default_changed(eCapture, eCommunications, None)` gives `flow Capture`, `role Communications`, and `device_id None`
  - `default_changed(eAll, eConsole, ..)` gives `None`
  - `property_changed("x", &PKEY_Device_FriendlyName).key == Some("{A45C254E-DF1C-4EFD-8020-67D146A850E0} 14")`
- [ ] Implement them, then run `cargo test` until it's green.
- [ ] Try the structured-enum alternative (see Design). Keep whichever gives the correct `index.d.ts` union after `pnpm build`.

### Task 3: Registry, COM client, `onDeviceEvent`

**Files:** Modify `src/events/mod.rs` (registry, `Subscription`, `subscribe`), `src/events/device.rs` (`DeviceNotifier`, `on_device_event`), `src/lib.rs`, maybe `Cargo.toml`. Test: `__test__/addon.test.mjs`.

**Consumes:** `com_guard`, `device_enumerator`, `to_napi_err` (com.rs/devices.rs) and the Task 2 helpers. **Produces:** `events::subscribe(env, Subscription) -> Result<Function<(), ()>>` and `Subscription::new(com: ComGuard, unregister: impl FnOnce() + 'static)`, both for PRs 9 and 10.

- [ ] Failing JS tests. Spawn children with `node:child_process` `spawnSync(process.execPath, ['--input-type=module', '-e', src], { timeout })`, where `src` imports the addon via an absolute `file://` URL to `index.js`:
  ```js
  test('onDeviceEvent returns an idempotent unsubscribe', () => {
    const unsubscribe = onDeviceEvent(() => {});
    assert.equal(typeof unsubscribe, 'function');
    unsubscribe(); unsubscribe();
  });
  test('onDeviceEvent rejects a non-function callback', () => {
    assert.throws(() => onDeviceEvent(42));
  });
  test('a process exits cleanly while still subscribed', () => {
    const r = child(`m.onDeviceEvent(() => {}); setTimeout(() => process.exit(0), 200);`);
    assert.equal(r.status, 0, r.stderr);
  });
  test('unsubscribing lets the event loop drain', () => {
    const r = child(`const u = m.onDeviceEvent(() => {}); setTimeout(u, 100);`, 5000);
    assert.equal(r.status, 0, r.stderr);
  });
  test('a live subscription keeps the event loop alive', () => {
    const r = child(`m.onDeviceEvent(() => {});`, 1000);
    assert.equal(r.signal, 'SIGTERM');  // killed by timeout = still alive
  });
  ```
  Gated live test (`JSCAW_TEST_DEVICE`, same gating as the existing `setDefaultDevice` live test): subscribe, record the current console default, `setDefaultDevice(JSCAW_TEST_DEVICE, ['console'])`, then await (≤3s, polling with `setTimeout`) a `defaultChanged` event with `role 'console'` and `deviceId === JSCAW_TEST_DEVICE`. Restore the original default and unsubscribe in `finally`.
- [ ] `pnpm build:debug && pnpm test`. It should fail with `onDeviceEvent` not exported.
- [ ] Implement the registry, `DeviceNotifier` and `on_device_event` per the Design section.
- [ ] `pnpm build && pnpm test && bun test` should be green. Also run the gated test locally with `JSCAW_TEST_DEVICE=<a non-default render id>`.

### Task 4: Docs, example, verification, commit

**Files:** Create `examples/device-events.ts` (it logs every event and unsubscribes on SIGINT). Modify `README.md` (an "Events → Device events" section with the union type, the loop-alive and unsubscribe note, the throwing-callback note, and "callbacks" removed from the v1 exclusions) and `AGENTS.md` (`events/` holds the shared subscription registry and device notifications).

- [ ] `cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test`, `bun test`
- [ ] Manual check: run the example, disable and re-enable a device in Sound settings or plug in headphones, and switch the default. Events should print. Ctrl+C should exit cleanly. Check `node -e "require('./index.js').onDeviceEvent(()=>{}); setTimeout(()=>process.exit(), 500)"` too.
- [ ] Commit with `feat: add device event notifications` (semantic-commits), push, and open the PR.

## Verification

All of Task 4's checks, plus a run of the gated `JSCAW_TEST_DEVICE` live test on a machine with two render devices.
