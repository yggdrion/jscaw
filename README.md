# jscaw

Windows-only native addon (Node-API via [napi-rs](https://napi.rs)) to list active Core
Audio sessions and control per-process volume/mute. Works from Node.js and Bun.

```ts
import { listSessions, setProcessMute, setProcessVolume } from 'jscaw';

listSessions();
// [{ pid: 1234, processName: 'Discord.exe', volume: 1, muted: false }, ...]

setProcessVolume('Discord.exe', 0.3); // returns the number of sessions updated
setProcessMute('Discord.exe', true);
```

See [`examples/basic.ts`](examples/basic.ts) for a runnable example.

## Scope

- **Windows only.** The package's `os` field is `["win32"]`; it won't install on macOS/Linux.
- **Active sessions only.** `listSessions()` reflects audio sessions that currently exist on
  the default render endpoint. A process with no active audio session (nothing played yet,
  or it already finished) won't appear.
- **Every matching session changes together.** `setProcessVolume`/`setProcessMute` match by
  exact, case-insensitive executable filename and apply to *all* sessions owned by that
  process — a process that owns more than one session (e.g. Discord's voice and
  notification sessions) has all of them updated in one call, and the return value is the
  count of sessions that changed.

## v1 exclusions

No audio routing, no master/device volume control, no level meters, no change
callbacks/notifications, and no audio playback. This package only enumerates existing
sessions and adjusts their per-process volume/mute.

## ePad handoff

This package is a drop-in replacement for `src/mixer.py` in ePad only — it covers the
per-process volume/mute mixing that `mixer.py` used to own. Soundpad integration and the
WebSocket protocol work in ePad are unrelated and remain separate, unaffected efforts.

## Releasing

See [`docs/RELEASING.md`](docs/RELEASING.md).
