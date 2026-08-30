# Windows Audio Sessions Implementation Plan

**Goal:** Publish a public Windows-only Node-API package for listing active audio sessions and changing per-process volume/mute. Bun must load it.

**Architecture:** TypeScript API -> Rust `napi-rs` addon -> `windows` crate -> Core Audio COM. npm distributes Windows x64 and ARM64 prebuilds.

## Constraints

- Confirm `@r4b2/win-audio-sessions` is available before any release.
- v1 supports only `listSessions`, `setProcessVolume`, and `setProcessMute`.
- No routing, master volume, meters, callbacks, or audio playback.
- Match every active session with an exact case-insensitive executable filename.
- Reject non-finite or out-of-range volume; return `0` when no session matches.
- Build and test on Windows under both Node and Bun. Release only in GitHub Actions.

## Task 1: Scaffold

- [x] In the repository root, scaffold the maintained napi-rs template with pnpm and retain only `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` targets. (`napi new`'s interactive min-node-api picker can't run under a non-TTY stdin, so the project was hand-written instead: `package.json`, `Cargo.toml`, `build.rs`, `src/lib.rs`, matching what the template generates.)
- [x] Set package metadata: name `@r4b2/win-audio-sessions`, MIT license, Node `>=20.17.0`, `os: ["win32"]`, and the actual GitHub repository URL.
- [x] Run `pnpm install`, `pnpm build`, and `pnpm test`; confirm the generated `.node` addon loads.
- [x] Commit `chore: scaffold native audio sessions package`.

## Task 2: Public API and safe tests

Create these exact exports and declarations:

```ts
export interface AudioSession {
  pid: number;
  processName: string;
  volume: number;
  muted: boolean;
}
export function listSessions(): AudioSession[];
export function setProcessVolume(processName: string, volume: number): number;
export function setProcessMute(processName: string, muted: boolean): number;
```

- [x] Write tests for session object shape, invalid `-0.1`, `1.1`, and `NaN` volume, and unknown process return value `0`.
- [x] Add `#[napi(object)] AudioSession` and three synchronous `#[napi]` functions. Validate volume before native access.
- [x] Run `pnpm build`, `pnpm test`, and `bun test`.
- [x] Commit `feat: define audio session API`.

## Task 3: Core Audio enumeration

- [x] Create `src/core_audio.rs`; add `windows` crate features `Win32_Foundation`, `Win32_Media_Audio`, `Win32_System_Com`, and `Win32_System_Threading`. (Also needed `Win32_System_Com_StructuredStorage` and `Win32_System_Variant` — `IMMDevice::Activate` is feature-gated behind both.)
- [x] Add pure Rust tests proving volume validation accepts `0.5` and rejects `-0.01` and `1.01`.
- [x] Implement this COM path per addon call:

```text
IMMDeviceEnumerator
→ GetDefaultAudioEndpoint(eRender, eConsole)
→ IAudioSessionManager2
→ IAudioSessionEnumerator
→ IAudioSessionControl2::GetProcessId
→ ISimpleAudioVolume::GetMasterVolume / GetMute
→ QueryFullProcessImageNameW
→ executable basename
```

- [x] Skip sessions without a resolvable process; convert actual Core Audio errors to `napi::Error` including HRESULT context.
- [x] Run `cargo fmt --check`, `cargo clippy -- -D warnings`, `pnpm test`, and `bun test`.
- [x] Commit `feat: list Windows audio sessions`.

## Task 4: Volume and mute mutation

- [x] Reuse enumeration and call `ISimpleAudioVolume::SetMasterVolume` or `SetMute` on all matching sessions.
- [x] Return the count of successfully updated sessions; return `0` when there are none.
- [x] Add an opt-in integration test gated by `WIN_AUDIO_SESSIONS_TEST_PROCESS`; normal CI must not change any real session.
- [x] Manually test a known audible process, restore its preferred volume, then run Node and Bun tests. (Ran live against `Discord.exe`, which has two sessions on this machine — confirmed both get updated and both restore correctly.)
- [x] Commit `feat: control per-process session volume`.

## Task 5: CI and releases

- [x] Keep the napi-rs generated CI workflow and limit native build jobs to Windows x64 and ARM64. (No CLI-generated workflow existed since Task 1 was hand-scaffolded — wrote `.github/workflows/CI.yml` from scratch, modeled on napi-rs's standard template, with build/test-node/test-bun/publish jobs for just `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` on `windows-latest`/`windows-11-arm` runners.)
- [x] Add Windows Node and Bun runtime test jobs. The publish job must wait for them and have `contents: write` plus `id-token: write` permissions.
- [x] Use the generated napi-rs optional-dependency packaging: platform packages publish first, then the root package with npm provenance. (`napi create-npm-dirs` generated `npm/win32-x64-msvc/` and `npm/win32-arm64-msvc/`; `napi pre-publish` wires `optionalDependencies` + copies binaries in at publish time — never committed.)
- [x] Create `docs/RELEASING.md`: create npm scope, configure `NPM_TOKEN` or npm trusted publishing, use protected Actions release workflow, and never publish manually.
- [x] Before the first release, run `pnpm napi prepublish -t npm --dry-run` and `npm pack --dry-run --ignore-scripts` only. (The actual command is `napi pre-publish`, not `prepublish` — fixed the `prepublishOnly` script to match. `npm pack --dry-run --ignore-scripts` ran clean. `napi pre-publish --dry-run` requires both platforms' `.node` binaries to exist first; this machine's Visual Studio install has the x64 C++ toolset but not the ARM64 one (`vswhere` confirms `Microsoft.VisualStudio.Component.VC.Tools.ARM64` is absent), so only the x64 binary could be built locally and the two-platform dry run couldn't complete here. Per Task 0's caveat, I didn't install that VS component unprompted — flagged it to the user instead. CI's native `windows-11-arm` runner isn't affected by this local gap.)
- [x] Commit `ci: build and publish Windows native binaries`.

## Task 6: Documentation and ePad handoff

- [ ] Add `examples/basic.ts` using `listSessions`, `setProcessVolume("Discord.exe", 0.3)`, and `setProcessMute("Discord.exe", false)`.
- [ ] Document: Windows-only; active sessions only; all matching sessions change; v1 exclusions.
- [ ] State that ePad will only replace `src/mixer.py`; Soundpad and WebSocket protocol work remain separate.
- [ ] Run `cargo fmt --check`, `cargo clippy -- -D warnings`, `pnpm build`, `pnpm test`, `bun test`, and `git diff --check`.
- [ ] Commit `docs: document Windows audio sessions package`.
