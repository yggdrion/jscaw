# PR 5: Peak Meters Implementation Plan

> **For agentic workers:** execute inline with superpowers:executing-plans + superpowers:test-driven-development. Steps use checkbox (`- [ ]`) syntax.

**First execution step:** copy this file to `docs/superpowers/plans/2026-10-06-pr-5-peak-meters.md` (repo convention) and include it in the commit.

**Goal:** Expose pycaw's `IAudioMeterInformation`: `getEndpointPeak(deviceId?) → number | null` and `getSessionPeak(target) → number[]`.

**Spec:** `docs/superpowers/specs/2026-10-04-pycaw-parity-roadmap-design.md` (PR 5 + "Cross-cutting decisions").

## Context

PRs 1–4 have merged (`b575b5f` is the tip of main). PR 5 depends only on PR 1. Nothing reads level meters today, and the README still lists "no level meters" under "v1 exclusions". The spec is already approved, so this session goes straight to the plan and then TDD.

## Design (shortest path, reusing existing helpers)

- **No `meter.rs`.** That's two functions and about 15 lines. The endpoint peak goes in `src/endpoint.rs`, which already does device resolve + activate. The session peak goes in `src/sessions.rs` because `map_matching` is private there. (This departs from the spec's module list. Add a `meter.rs` only if meters grow, e.g. per-channel peaks.)
- **Endpoint:** generalise `with_endpoint` in `src/endpoint.rs:42` into a private `with_activated<I: Interface, T>(device_id, context, f)`. Keep `with_endpoint` as a one-line wrapper (`with_activated::<IAudioEndpointVolume, _>(…, "failed to activate endpoint volume", f)`) so the 6 existing call sites don't change.
  ```rust
  pub fn get_peak(device_id: Option<&str>) -> Result<Option<f64>> {
      with_activated::<IAudioMeterInformation, _>(device_id, "failed to activate peak meter", |m| {
          unsafe { m.GetPeakValue() }.map(f64::from)
              .map_err(|e| to_napi_err("failed to read endpoint peak", e))
      })
  }
  ```
  `IAudioMeterInformation` lives in `windows::Win32::Media::Audio::Endpoints`, which is already enabled. No Cargo changes.
- **Session:** works like `get_channel_volumes` (`src/sessions.rs`):
  ```rust
  /// One 0..1 peak per matching session; sessions without a meter are skipped.
  pub fn get_peak(target: SessionTarget) -> Result<Vec<f64>> {
      map_matching(target, |control| unsafe {
          control.cast::<IAudioMeterInformation>().ok()?.GetPeakValue().ok().map(f64::from)
      })
  }
  ```
- **`src/lib.rs`:** two thin `#[napi]` wrappers, `get_endpoint_peak(device_id: Option<String>)` and `get_session_peak(target: SessionTarget)`.
- **Semantics** (inherited, no new code): a missing or unknown device gives `null`/`[]`. Malformed targets throw via `into_matcher`. Unmatched targets give `[]`. The value is 0 when nothing is playing. A capture endpoint's meter only moves while some app has a capture stream open. Document this in the README.

Skipped: per-channel peaks (`GetChannelsPeakValues`) and `QueryHardwareSupport` on the meter. The spec doesn't ask for them, and `getEndpointVolume().hardwareSupport` already reports the meter bit.

## Tasks

### Task 1: Native + tests (TDD)

**Files:** `src/endpoint.rs`, `src/sessions.rs`, `src/lib.rs`, `__test__/addon.test.mjs`

- [ ] Add `getEndpointPeak` and `getSessionPeak` to the import list in `__test__/addon.test.mjs`. Append these tests, reusing `STATES`, `BAD_ID`, `BAD_TARGETS` and `UNMATCHED_TARGETS`:
  - `getEndpointPeak returns a 0..1 number or null`: `null` ⇔ `getDefaultDevice() === null`, otherwise `typeof number`, `0 <= p <= 1`. Do the same for the default capture device when one exists.
  - `getEndpointPeak returns null for unknown ids`: the zero-GUID id, `'not-a-device-id'` and `''`.
  - `getEndpointPeak never throws for devices in any state`: sweep `listDevices({ state: STATES })`.
  - `getSessionPeak rejects malformed targets and matches nothing cleanly`: `BAD_TARGETS` throw `/exactly one/`, and `UNMATCHED_TARGETS` give `[]`.
  - `getSessionPeak returns one 0..1 peak per listed session`: for each `listSessions({ includeSystemSounds: true })` session, `getSessionPeak({ instanceId })` has length ≤ 1 and values in 0..1.
- [ ] `pnpm build:debug && pnpm test`. The new tests should fail (the exports are missing).
- [ ] Implement the design above.
- [ ] `pnpm build:debug && pnpm test` should pass. Also run `cargo fmt` and `cargo test`.

There are no Rust unit tests because there's no new pure helper (YAGNI). Every path is read-only, so no gated live test is needed.

### Task 2: Docs + example

- [ ] `README.md`: add a `## Peak meters` section after Sessions (import, two calls, the semantics note). Drop "no level meters" from "v1 exclusions". Add `examples/meters.ts` to the examples list. Update the top description line to mention meters.
- [ ] `examples/meters.ts`: sample `getEndpointPeak()` and `getSessionPeak({ instanceId })` for each `listSessions()` entry, 10 times at 100 ms (`setTimeout` loop), printing a bar per line. Read-only.
- [ ] `AGENTS.md`: the module-layout sentence. `endpoint.rs` gains "and peak meter", and `sessions.rs` gains "peak".

### Task 3: Verify + commit + PR

- [ ] `cargo fmt --check`, `cargo test`, `pnpm build`, `pnpm test` and `bun test` should all be green (superpowers:verification-before-completion).
- [ ] Manual: play audio and run `node --experimental-strip-types examples/meters.ts` (or `bun examples/meters.ts`). The bars should move, and they should read 0 when paused.
- [ ] Commit `feat: add endpoint and session peak meters`, with no co-author trailers. Push the branch and open the PR (title as the commit, body: behavior, win32 x64/arm64, commands run). Then bind it with the ccd_pr tools.

## Verification

Covered in Task 3. The key review points are: an unknown device gives `null` with no throw, the any-state device sweep doesn't throw, a malformed target throws, and the existing endpoint tests still pass after the `with_activated` refactor.
