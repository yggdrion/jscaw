# jscaw

Windows-only native addon (Node-API via [napi-rs](https://napi.rs)) to list audio devices
and active Core Audio sessions, switch the default device, control device (master) and
per-process volume/mute, read peak levels, and watch for device changes. Works from Node.js and Bun.

```ts
import { listSessions, setProcessMute, setProcessVolume } from 'jscaw';

listSessions();
// [{ pid: 1234, processName: 'Discord.exe', volume: 1, muted: false, state: 'active', displayName: '', … }, ...]

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

### Device properties

```ts
import { getDeviceProperties } from 'jscaw';

getDeviceProperties(id); // null when the id is unknown
// { '{A45C254E-DF1C-4EFD-8020-67D146A850E0} 14': 'Speakers (Realtek(R) Audio)',
//   '{B3F8FA53-0004-438E-9003-51A46E139BFC} 6': 'Realtek(R) Audio', … }
```

This is the device's whole property store, with keys formatted `"{FMTID} pid"` exactly like
pycaw's `AudioDevice.properties`. Strings, booleans and 32/64-bit integers are decoded, and
CLSID values come back as braced GUID strings. Values of any other type are `null`.
Properties that can't be read are left out.

## Default device

```ts
import { setDefaultDevice } from 'jscaw';

setDefaultDevice(id); // all roles: 'console', 'multimedia' and 'communications'
setDefaultDevice(id, ['communications']); // only the default comms device
```

Returns `false` when `id` is not a known device, and throws when Windows rejects the switch
(e.g. a disabled device). Works for capture devices too. This relies on the undocumented
`IPolicyConfig` Windows API (the one the Sound control panel uses), so it needs Windows 10+.

## Device events

```ts
import { onDeviceEvent } from 'jscaw';

const unsubscribe = onDeviceEvent((event) => {
  // { type: 'added' | 'removed', deviceId }
  // { type: 'stateChanged', deviceId, state }
  // { type: 'defaultChanged', flow, role, deviceId } — deviceId is null when no default is left
  // { type: 'propertyChanged', deviceId, key } — key as in getDeviceProperties
  console.log(event);
});
unsubscribe(); // idempotent
```

This is pycaw's `MMNotificationClient`. Events arrive asynchronously on the JS thread.
Like `fs.watch`, a live subscription keeps the process running until you call
`unsubscribe()`. Exiting while subscribed is safe, because subscriptions are cleaned up on
exit. An exception thrown from the callback is uncaught, as it would be from an
`EventEmitter` listener.

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

### Endpoint volume events

```ts
import { onEndpointVolumeChange } from 'jscaw';

const unsubscribe = onEndpointVolumeChange((event) => {
  // { volume: 0.42, muted: false, channelVolumes: [0.42, 0.42], selfInitiated: false }
  console.log(event);
}, micId); // deviceId optional: default speakers when omitted
unsubscribe?.(); // null when the device doesn't exist
```

This is pycaw's `AudioEndpointVolumeCallback`. `selfInitiated` is `true` when a jscaw setter in
this process made the change (every setter tags its changes with a per-process event context),
and `false` for changes from the Windows mixer, media keys or other apps. Subscriptions behave
like `onDeviceEvent`: they keep the process alive until `unsubscribe()` and are cleaned up on exit.

## Sessions

```ts
import {
  getSessionChannelVolumes,
  listSessions,
  setSessionChannelVolume,
  setSessionDisplayName,
  setSessionDuckingPreference,
  setSessionGroupingParam,
  setSessionIconPath,
  setSessionMute,
  setSessionVolume,
} from 'jscaw';

listSessions({ deviceId, includeSystemSounds: true });
// [{ pid, processName, volume, muted, state: 'active', displayName: '', iconPath: '',
//    groupingParam: '{39E4403C-…}', sessionId: '…', instanceId: '…', isSystemSounds: false }, ...]
setSessionVolume({ processName: 'Discord.exe' }, 0.3); // every matching session
setSessionMute({ pid: 1234 }, true);
setSessionDisplayName({ instanceId }, 'Game audio');
setSessionIconPath({ instanceId }, 'C:\\game\\icon.ico');
setSessionGroupingParam({ instanceId }, '{6A1D3B2C-0000-4000-8000-00000000C0DE}');
setSessionDuckingPreference({ processName: 'Spotify.exe' }, true); // opt out of ducking
getSessionChannelVolumes({ processName: 'Discord.exe' }); // [[1, 1], [0.5, 0.5]] — one array per session
setSessionChannelVolume({ instanceId }, 0, 0.5); // left channel to 50%
```

`listSessions` defaults to the default render device and hides the system sounds session
(`pid: 0`, `processName: ''`, `isSystemSounds: true`) unless `includeSystemSounds` is set.
`state` is `'inactive'`, `'active'` or `'expired'`. `displayName`/`iconPath` are returned as
Windows stores them, which may be empty or an indirect `@…` resource string. `groupingParam`
is a braced GUID; sessions sharing one are grouped in the volume mixer.

Setters take a `SessionTarget`: exactly one of `{ pid }`, `{ processName }` (case-insensitive)
or `{ instanceId }`, plus an optional `deviceId`. Anything else throws. Each returns the number
of sessions changed, `0` when nothing matches or the device is missing.
`setProcessVolume(name, v)`/`setProcessMute(name, m)` are shorthands for `{ processName }`.
Channel volumes are 0..1 scalars relative to the session volume; `setSessionChannelVolume`
skips (and doesn't count) sessions that don't have the requested channel.

### Session events

```ts
import { onDuckEvent, onSessionCreated, onSessionEvent } from 'jscaw';

onSessionCreated((session) => console.log(session)); // same shape as listSessions(), deviceId optional
onSessionEvent({ processName: 'Discord.exe' }, (event) => {
  // { type: 'volumeChanged', instanceId: '…', volume: 0.3, muted: false, selfInitiated: false }
  console.log(event);
});
onDuckEvent((event) => console.log(event)); // { type: 'duck', instanceId: '…', activeSessionCount: 1 }
```

These are pycaw's `AudioSessionNotification`, `AudioSessionEvents` and
`IAudioVolumeDuckNotification`. `onSessionEvent` events are `displayNameChanged`,
`iconPathChanged`, `volumeChanged`, `channelVolumeChanged` (`changedChannel` is `null` when every
channel changed), `groupingChanged`, `stateChanged` and `disconnected` (with a `reason`). Each
carries the `instanceId` of the session that fired, and the change events carry `selfInitiated`
like `onEndpointVolumeChange`. Duck events report the communications session that caused the
ducking.

`onSessionEvent` binds to the sessions matching its target when it's called; sessions created
later aren't added, so pair it with `onSessionCreated` to follow an app. A session that expires
or disconnects stays subscribed until you call `unsubscribe()`. All three return `null` when the
device doesn't exist (or, for `onSessionEvent`, when nothing matches), and otherwise behave like
`onDeviceEvent`: they keep the process alive until `unsubscribe()` and are cleaned up on exit.

## Peak meters

```ts
import { getEndpointPeak, getSessionPeak } from 'jscaw';

getEndpointPeak(); // 0..1 peak of the default speakers, or null when there is no device
getEndpointPeak(micId); // capture devices too
getSessionPeak({ processName: 'Discord.exe' }); // [0.31, 0] — one peak per matching session
```

Peaks are instantaneous 0..1 samples, so poll them for a level meter. They read `0` while
nothing is playing, and a microphone's meter only moves while some app is capturing from it.
`getEndpointPeak` takes the same optional `deviceId` as the endpoint volume functions;
`getSessionPeak` takes a `SessionTarget` and returns `[]` when nothing matches.

See [`examples/basic.ts`](examples/basic.ts), [`examples/devices.ts`](examples/devices.ts),
[`examples/device-properties.ts`](examples/device-properties.ts),
[`examples/default-device.ts`](examples/default-device.ts),
[`examples/device-events.ts`](examples/device-events.ts),
[`examples/endpoint-volume.ts`](examples/endpoint-volume.ts),
[`examples/endpoint-volume-events.ts`](examples/endpoint-volume-events.ts),
[`examples/sessions.ts`](examples/sessions.ts),
[`examples/session-events.ts`](examples/session-events.ts) and [`examples/meters.ts`](examples/meters.ts)
for runnable examples.

## Scope

- **Windows only.** The package's `os` field is `["win32"]`; it won't install on macOS/Linux.
- **Existing sessions only.** `listSessions()` reflects audio sessions that currently exist
  on one device (the default render endpoint unless `deviceId` is given). A process with no
  audio session (nothing played yet), or whose process has exited, won't appear.
- **Every matching session changes together.** `setProcessVolume`/`setProcessMute` (and
  `{ processName }` targets) match by exact, case-insensitive executable filename and apply
  to *all* sessions owned by that process — a process that owns more than one session (e.g. Discord's voice and
  notification sessions) has all of them updated in one call, and the return value is the
  count of sessions that changed.

## v1 exclusions

No audio playback.

## Releasing

See [`docs/RELEASING.md`](docs/RELEASING.md).
