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

- [ ] In the repository root, scaffold the maintained napi-rs template with pnpm and retain only `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` targets.
- [ ] Set package metadata: name `@r4b2/win-audio-sessions`, MIT license, Node `>=20.17.0`, `os: ["win32"]`, and the actual GitHub repository URL.
- [ ] Run `pnpm install`, `pnpm build`, and `pnpm test`; confirm the generated `.node` addon loads.
- [ ] Commit `chore: scaffold native audio sessions package`.

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

- [ ] Write tests for session object shape, invalid `-0.1`, `1.1`, and `NaN` volume, and unknown process return value `0`.
- [ ] Add `#[napi(object)] AudioSession` and three synchronous `#[napi]` functions. Validate volume before native access.
- [ ] Run `pnpm build`, `pnpm test`, and `bun test`.
- [ ] Commit `feat: define audio session API`.

## Task 3: Core Audio enumeration

- [ ] Create `src/core_audio.rs`; add `windows` crate features `Win32_Foundation`, `Win32_Media_Audio`, `Win32_System_Com`, and `Win32_System_Threading`.
- [ ] Add pure Rust tests proving volume validation accepts `0.5` and rejects `-0.01` and `1.01`.
- [ ] Implement this COM path per addon call:

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

- [ ] Skip sessions without a resolvable process; convert actual Core Audio errors to `napi::Error` including HRESULT context.
- [ ] Run `cargo fmt --check`, `cargo clippy -- -D warnings`, `pnpm test`, and `bun test`.
- [ ] Commit `feat: list Windows audio sessions`.

## Task 4: Volume and mute mutation

- [ ] Reuse enumeration and call `ISimpleAudioVolume::SetMasterVolume` or `SetMute` on all matching sessions.
- [ ] Return the count of successfully updated sessions; return `0` when there are none.
- [ ] Add an opt-in integration test gated by `WIN_AUDIO_SESSIONS_TEST_PROCESS`; normal CI must not change any real session.
- [ ] Manually test a known audible process, restore its preferred volume, then run Node and Bun tests.
- [ ] Commit `feat: control per-process session volume`.

## Task 5: CI and releases

- [ ] Keep the napi-rs generated CI workflow and limit native build jobs to Windows x64 and ARM64.
- [ ] Add Windows Node and Bun runtime test jobs. The publish job must wait for them and have `contents: write` plus `id-token: write` permissions.
- [ ] Use the generated napi-rs optional-dependency packaging: platform packages publish first, then the root package with npm provenance.
- [ ] Create `docs/RELEASING.md`: create npm scope, configure `NPM_TOKEN` or npm trusted publishing, use protected Actions release workflow, and never publish manually.
- [ ] Before the first release, run `pnpm napi prepublish -t npm --dry-run` and `npm pack --dry-run --ignore-scripts` only.
- [ ] Commit `ci: build and publish Windows native binaries`.

## Task 6: Documentation and ePad handoff

- [ ] Add `examples/basic.ts` using `listSessions`, `setProcessVolume("Discord.exe", 0.3)`, and `setProcessMute("Discord.exe", false)`.
- [ ] Document: Windows-only; active sessions only; all matching sessions change; v1 exclusions.
- [ ] State that ePad will only replace `src/mixer.py`; Soundpad and WebSocket protocol work remain separate.
- [ ] Run `cargo fmt --check`, `cargo clippy -- -D warnings`, `pnpm build`, `pnpm test`, `bun test`, and `git diff --check`.
- [ ] Commit `docs: document Windows audio sessions package`.
