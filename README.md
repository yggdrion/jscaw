# jscaw

Windows-only native addon (Node-API via [napi-rs](https://napi.rs)) to list audio devices
and active Core Audio sessions, and control device (master) and per-process volume/mute. Works from Node.js and Bun.

```ts
import { listSessions, setProcessMute, setProcessVolume } from 'jscaw';

listSessions();
// [{ pid: 1234, processName: 'Discord.exe', volume: 1, muted: false }, ...]

setProcessVolume('Discord.exe', 0.3); // returns the number of sessions updated
setProcessMute('Discord.exe', true);
```

## Devices

```ts
import { getDefaultDevice, getDevice, listDevices } from 'jscaw';

listDevices(); // active render + capture devices
// [{ id: '{0.0.0.00000000}.{…}', name: 'Speakers (Realtek(R) Audio)', flow: 'render', state: 'active' }, ...]
listDevices({ flow: 'capture', state: ['active', 'unplugged'] });
getDefaultDevice(); // default speakers, or null
getDefaultDevice('capture', 'communications'); // default comms microphone, or null
getDevice(id); // null when the id is unknown
```

`listDevices` defaults to `flow: 'all'` and `state: ['active']`; `state` accepts any of
`'active'`, `'disabled'`, `'notPresent'` and `'unplugged'`. `getDefaultDevice` defaults to
`'render'` and the `'console'` role. `name` is the device's friendly name, falling back to its
description. Machines with no audio devices get `[]`/`null` rather than errors.

## Endpoint volume

```ts
import {
  getDefaultDevice,
  getEndpointVolume,
  setEndpointChannelVolume,
  setEndpointMute,
  setEndpointVolume,
  setEndpointVolumeDb,
  stepEndpointVolume,
} from 'jscaw';

const micId = getDefaultDevice('capture')?.id;
getEndpointVolume(); // default speakers, or null
// { volume: 0.42, volumeDb: -14.3, muted: false, channels: [{ volume: 0.42, volumeDb: -14.3 }, …],
//   range: { minDb: -65.25, maxDb: 0, incrementDb: 0.03 }, step: { current: 21, count: 51 }, hardwareSupport: 0 }
setEndpointVolume(0.3); // true when applied, false when the device doesn't exist
setEndpointVolumeDb(-20, micId); // validated against `range`
setEndpointMute(true, micId); // works for capture devices (microphones) too
setEndpointChannelVolume(0, 0.5); // channel index < channels.length
stepEndpointVolume('up');
```

Every function takes an optional trailing `deviceId` (from `listDevices()`); without it the
default render (`console`) device is used. Setters return `false` instead of throwing when the
device is missing, disabled or unplugged; invalid values throw.

See [`examples/basic.ts`](examples/basic.ts), [`examples/devices.ts`](examples/devices.ts)
and [`examples/endpoint-volume.ts`](examples/endpoint-volume.ts) for runnable examples.

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

No audio routing (devices can be listed, not switched), no level meters, no change
callbacks/notifications, and no audio playback.

## Releasing

See [`docs/RELEASING.md`](docs/RELEASING.md).
