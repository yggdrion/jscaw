import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  getDefaultDevice,
  getDevice,
  getDeviceProperties,
  getEndpointPeak,
  getEndpointVolume,
  getSessionChannelVolumes,
  getSessionPeak,
  listDevices,
  listSessions,
  onDeviceEvent,
  setDefaultDevice,
  setEndpointChannelVolume,
  setEndpointMute,
  setEndpointVolume,
  setEndpointVolumeDb,
  setProcessMute,
  setProcessVolume,
  setSessionChannelVolume,
  setSessionDisplayName,
  setSessionDuckingPreference,
  setSessionGroupingParam,
  setSessionIconPath,
  setSessionMute,
  setSessionVolume,
  stepEndpointVolume,
} from '../index.js';

const FLOWS = ['render', 'capture'];
const STATES = ['active', 'disabled', 'notPresent', 'unplugged'];
const SESSION_STATES = ['inactive', 'active', 'expired'];
const GUID_RE = /^\{[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\}$/;
const assertSession = (s) => {
  for (const key of ['pid', 'volume']) assert.equal(typeof s[key], 'number', key);
  for (const key of ['processName', 'displayName', 'iconPath', 'sessionId', 'instanceId']) {
    assert.equal(typeof s[key], 'string', key);
  }
  assert.equal(typeof s.muted, 'boolean');
  assert.equal(typeof s.isSystemSounds, 'boolean');
  assert.ok(SESSION_STATES.includes(s.state), `bad state ${s.state}`);
  assert.ok(s.groupingParam === '' || GUID_RE.test(s.groupingParam), s.groupingParam);
};

test('listSessions returns rich sessions without system sounds by default', () => {
  const sessions = listSessions();
  assert.ok(Array.isArray(sessions));
  for (const s of sessions) {
    assertSession(s);
    assert.equal(s.isSystemSounds, false);
    assert.ok(s.processName.length > 0);
  }
});

test('listSessions includeSystemSounds adds only the system sounds session', () => {
  const all = listSessions({ includeSystemSounds: true });
  all.forEach(assertSession);
  const system = all.filter((s) => s.isSystemSounds);
  assert.ok(system.length <= 1);
  for (const s of system) assert.equal(s.processName, '');
  assert.ok(all.length >= listSessions().length);
});

test('listSessions returns [] for unknown devices', () => {
  assert.deepEqual(listSessions({ deviceId: 'not-a-device-id' }), []);
  assert.deepEqual(listSessions({ deviceId: '' }), []);
});

test('listSessions never throws for devices in any state', () => {
  for (const device of listDevices({ state: STATES })) {
    listSessions({ deviceId: device.id, includeSystemSounds: true }).forEach(assertSession);
  }
});

test('setProcessVolume rejects out-of-range and non-finite volume', () => {
  assert.throws(() => setProcessVolume('nope.exe', -0.1));
  assert.throws(() => setProcessVolume('nope.exe', 1.1));
  assert.throws(() => setProcessVolume('nope.exe', NaN));
});

test('setProcessVolume returns 0 for an unmatched process', () => {
  assert.equal(setProcessVolume('does-not-exist.exe', 0.5), 0);
});

test('setProcessMute returns 0 for an unmatched process', () => {
  assert.equal(setProcessMute('does-not-exist.exe', true), 0);
});

// Opt-in: mutates a real audio session, so it must never run unattended in CI.
// Set JSCAW_TEST_PROCESS to the executable name of a currently-playing app to run it.
test('setProcessVolume/setProcessMute update a real session and can be restored', (t) => {
  const targetProcess = process.env.JSCAW_TEST_PROCESS;
  if (!targetProcess) {
    t.skip('set JSCAW_TEST_PROCESS to a running process name to run this check');
    return;
  }

  // A process can own multiple sessions (e.g. Discord: voice + notifications), and the
  // addon updates all of them, so match on the full set rather than assuming exactly one.
  const findSessions = () =>
    listSessions().filter((s) => s.processName.toLowerCase() === targetProcess.toLowerCase());

  const before = findSessions();
  assert.ok(before.length > 0, `no active audio session found for ${targetProcess}`);

  try {
    assert.equal(setProcessVolume(targetProcess, 0.3), before.length);
    for (const session of findSessions()) {
      assert.ok(Math.abs(session.volume - 0.3) < 0.01);
    }

    assert.equal(setProcessMute(targetProcess, true), before.length);
    assert.ok(findSessions().every((s) => s.muted === true));
    assert.equal(setProcessMute(targetProcess, false), before.length);
    assert.ok(findSessions().every((s) => s.muted === false));
  } finally {
    for (const session of before) {
      setProcessVolume(session.processName, session.volume);
      setProcessMute(session.processName, session.muted);
    }
  }
});

const BAD_TARGETS = [
  {},
  { deviceId: 'x' },
  { pid: 1, processName: 'x.exe' },
  { processName: 'x.exe', instanceId: 'y' },
  { pid: 1, instanceId: 'y' },
];
const UNMATCHED_TARGETS = [
  { pid: 4294967290 },
  { processName: 'does-not-exist.exe' },
  { instanceId: 'no-such-instance' },
  { processName: 'does-not-exist.exe', deviceId: 'not-a-device-id' },
];

test('session setters reject malformed targets', () => {
  for (const target of BAD_TARGETS) {
    assert.throws(() => setSessionVolume(target, 0.5), /exactly one/);
    assert.throws(() => setSessionMute(target, true), /exactly one/);
  }
  assert.throws(() => setSessionVolume(null, 0.5));
  for (const pid of [NaN, Infinity, -1, 1.5, 2 ** 32]) {
    assert.throws(() => setSessionMute({ pid }, true), /pid must be/);
  }
});

test('setSessionVolume rejects invalid volume even when nothing matches', () => {
  for (const v of [-0.1, 1.1, NaN]) {
    assert.throws(() => setSessionVolume({ processName: 'does-not-exist.exe' }, v));
  }
});

test('session setters return 0 when nothing matches', () => {
  for (const target of UNMATCHED_TARGETS) {
    assert.equal(setSessionVolume(target, 0.5), 0);
    assert.equal(setSessionMute(target, true), 0);
  }
});

const METADATA_SETTERS = [
  [() => setSessionDisplayName, 'name'],
  [() => setSessionIconPath, 'C:\\icon.ico'],
  [() => setSessionGroupingParam, '{6A1D3B2C-0000-4000-8000-00000000C0DE}'],
  [() => setSessionDuckingPreference, true],
];

test('metadata setters reject malformed targets and return 0 when nothing matches', () => {
  for (const [fn, value] of METADATA_SETTERS) {
    for (const target of BAD_TARGETS) assert.throws(() => fn()(target, value), /exactly one/);
    for (const target of UNMATCHED_TARGETS) assert.equal(fn()(target, value), 0);
  }
});

test('setSessionGroupingParam accepts any GUID casing/bracing and rejects non-GUIDs', () => {
  const target = { processName: 'does-not-exist.exe' };
  for (const g of ['6a1d3b2c-0000-4000-8000-00000000c0de', '{6a1d3b2c-0000-4000-8000-00000000c0de}']) {
    assert.equal(setSessionGroupingParam(target, g), 0);
  }
  for (const g of ['', 'not-a-guid', '{6a1d3b2c-0000-4000-8000-00000000c0de', '6a1d3b2c00004000800000000000c0de']) {
    assert.throws(() => setSessionGroupingParam(target, g), /GUID/);
  }
});

test('session setters target by instanceId and pid and can be restored', (t) => {
  const targetProcess = process.env.JSCAW_TEST_PROCESS;
  if (!targetProcess) {
    t.skip('set JSCAW_TEST_PROCESS to a running process name to run this check');
    return;
  }
  const s = listSessions().find((x) => x.processName.toLowerCase() === targetProcess.toLowerCase());
  assert.ok(s, `no active audio session found for ${targetProcess}`);
  const target = { instanceId: s.instanceId };
  const find = () => listSessions().find((x) => x.instanceId === s.instanceId);
  try {
    assert.equal(setSessionVolume(target, 0.25), 1);
    assert.ok(Math.abs(find().volume - 0.25) < 0.01);
    assert.equal(setSessionMute(target, !s.muted), 1);
    assert.equal(find().muted, !s.muted);
    assert.ok(setSessionMute({ pid: s.pid }, s.muted) >= 1);
    assert.equal(find().muted, s.muted);
    assert.equal(setSessionDisplayName(target, 'jscaw test'), 1);
    assert.equal(find().displayName, 'jscaw test');
    const group = '{6A1D3B2C-0000-4000-8000-00000000C0DE}';
    assert.equal(setSessionGroupingParam(target, group.toLowerCase()), 1);
    assert.equal(find().groupingParam, group);
  } finally {
    setSessionVolume(target, s.volume);
    setSessionMute(target, s.muted);
    setSessionDisplayName(target, s.displayName);
    if (s.groupingParam) setSessionGroupingParam(target, s.groupingParam);
  }
});

test('session channel volume rejects malformed targets and matches nothing cleanly', () => {
  for (const target of BAD_TARGETS) {
    assert.throws(() => getSessionChannelVolumes(target), /exactly one/);
    assert.throws(() => setSessionChannelVolume(target, 0, 0.5), /exactly one/);
  }
  for (const target of UNMATCHED_TARGETS) {
    assert.deepEqual(getSessionChannelVolumes(target), []);
    assert.equal(setSessionChannelVolume(target, 0, 0.5), 0);
  }
});

test('setSessionChannelVolume rejects invalid volume and channel even when nothing matches', () => {
  const target = { processName: 'does-not-exist.exe' };
  for (const v of [-0.1, 1.1, NaN]) assert.throws(() => setSessionChannelVolume(target, 0, v));
  for (const ch of [NaN, -1, 1.5, 2 ** 32]) {
    assert.throws(() => setSessionChannelVolume(target, ch, 0.5), /channel must be/);
  }
});

test('getSessionChannelVolumes returns per-channel scalars for listed sessions', () => {
  for (const s of listSessions({ includeSystemSounds: true })) {
    const result = getSessionChannelVolumes({ instanceId: s.instanceId });
    assert.ok(result.length <= 1);
    for (const channels of result) {
      for (const v of channels) assert.ok(v >= 0 && v <= 1, `bad channel volume ${v}`);
    }
  }
});

// Opt-in: mutates a real session's channel volume.
test('setSessionChannelVolume updates a real session and can be restored', (t) => {
  const targetProcess = process.env.JSCAW_TEST_PROCESS;
  if (!targetProcess) {
    t.skip('set JSCAW_TEST_PROCESS to a running process name to run this check');
    return;
  }
  const s = listSessions().find((x) => x.processName.toLowerCase() === targetProcess.toLowerCase());
  assert.ok(s, `no active audio session found for ${targetProcess}`);
  const target = { instanceId: s.instanceId };
  const [before] = getSessionChannelVolumes(target);
  assert.ok(before?.length > 0, 'session reports no channels');
  try {
    assert.equal(setSessionChannelVolume(target, 0, 0.2), 1);
    assert.ok(Math.abs(getSessionChannelVolumes(target)[0][0] - 0.2) < 0.01);
    assert.equal(setSessionChannelVolume(target, before.length, 0.5), 0); // out-of-range channel
  } finally {
    before.forEach((v, i) => setSessionChannelVolume(target, i, v));
  }
});

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

test('getDeviceProperties returns null for unknown ids', () => {
  assert.equal(getDeviceProperties('{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}'), null);
  assert.equal(getDeviceProperties('not-a-device-id'), null);
  assert.equal(getDeviceProperties(''), null);
});

test('getDeviceProperties returns pycaw-keyed decoded values', () => {
  const FRIENDLY_NAME = '{A45C254E-DF1C-4EFD-8020-67D146A850E0} 14';
  for (const device of listDevices()) {
    const props = getDeviceProperties(device.id);
    assert.equal(typeof props, 'object');
    assert.notEqual(props, null);
    for (const [key, value] of Object.entries(props)) {
      assert.match(key, /^\{[0-9A-F-]{36}\} \d+$/);
      assert.ok(
        value === null || ['string', 'number', 'boolean'].includes(typeof value),
        `${key}: ${typeof value}`,
      );
    }
    if (FRIENDLY_NAME in props) assert.equal(props[FRIENDLY_NAME], device.name);
  }
});

test('getDevice round-trips ids from listDevices', () => {
  for (const device of listDevices({ state: STATES })) {
    assert.deepEqual(getDevice(device.id), device);
  }
});

const assertEndpointVolume = (ev) => {
  for (const key of ['volume', 'volumeDb', 'hardwareSupport']) {
    assert.equal(typeof ev[key], 'number', key);
  }
  assert.ok(ev.volume >= 0 && ev.volume <= 1);
  assert.equal(typeof ev.muted, 'boolean');
  assert.ok(Array.isArray(ev.channels));
  for (const ch of ev.channels) {
    assert.equal(typeof ch.volume, 'number');
    assert.equal(typeof ch.volumeDb, 'number');
  }
  assert.ok(ev.range.minDb <= ev.range.maxDb);
  assert.equal(typeof ev.range.incrementDb, 'number');
  assert.ok(ev.step.current < Math.max(ev.step.count, 1));
};

test('getEndpointVolume returns the default render endpoint volume or null', () => {
  const ev = getEndpointVolume();
  if (ev === null) {
    assert.equal(getDefaultDevice(), null);
    return;
  }
  assertEndpointVolume(ev);
});

test('getEndpointVolume works for the default capture device', () => {
  const mic = getDefaultDevice('capture');
  if (mic === null) return; // no microphone (e.g. CI)
  assertEndpointVolume(getEndpointVolume(mic.id));
});

test('getEndpointVolume returns null for unknown ids', () => {
  assert.equal(getEndpointVolume('{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}'), null);
  assert.equal(getEndpointVolume('not-a-device-id'), null);
  assert.equal(getEndpointVolume(''), null);
});

test('getEndpointVolume never throws for devices in any state', () => {
  for (const device of listDevices({ state: STATES })) {
    const ev = getEndpointVolume(device.id);
    if (ev !== null) assertEndpointVolume(ev);
  }
});

const BAD_ID = 'not-a-device-id';

test('endpoint setters reject invalid arguments even for unknown devices', () => {
  for (const v of [-0.1, 1.1, NaN]) {
    assert.throws(() => setEndpointVolume(v, BAD_ID));
    assert.throws(() => setEndpointChannelVolume(0, v, BAD_ID));
  }
  for (const db of [NaN, Infinity, -Infinity]) {
    assert.throws(() => setEndpointVolumeDb(db, BAD_ID));
  }
  assert.throws(() => stepEndpointVolume('sideways', BAD_ID));
  for (const ch of [NaN, -1, 1.5, 2 ** 32]) {
    assert.throws(() => setEndpointChannelVolume(ch, 0.5, BAD_ID), /channel must be/);
  }
});

test('endpoint setters return false for unknown devices', () => {
  assert.equal(setEndpointVolume(0.5, BAD_ID), false);
  assert.equal(setEndpointVolumeDb(0, BAD_ID), false);
  assert.equal(setEndpointMute(true, BAD_ID), false);
  assert.equal(setEndpointChannelVolume(0, 0.5, BAD_ID), false);
  assert.equal(stepEndpointVolume('up', BAD_ID), false);
});

test('endpoint setters reject out-of-range dB and channel on a real device', () => {
  const ev = getEndpointVolume();
  if (ev === null) return; // no default render device (e.g. CI)
  assert.throws(() => setEndpointVolumeDb(ev.range.maxDb + 1), /volumeDb must be between/);
  assert.throws(() => setEndpointVolumeDb(ev.range.minDb - 1), /volumeDb must be between/);
  assert.throws(() => setEndpointChannelVolume(ev.channels.length, 0.5), /out of range/);
  assert.throws(() => setEndpointChannelVolume(-1, 0.5));
});

// Opt-in: mutates a real device's volume. Set JSCAW_TEST_DEVICE to a device id from listDevices().
test('endpoint setters update a real device and can be restored', (t) => {
  const deviceId = process.env.JSCAW_TEST_DEVICE;
  if (!deviceId) {
    t.skip('set JSCAW_TEST_DEVICE to a device id to run this check');
    return;
  }
  const before = getEndpointVolume(deviceId);
  assert.ok(before, `no endpoint volume for ${deviceId}`);
  const close = (a, b, eps = 0.01) => Math.abs(a - b) < eps;

  try {
    assert.equal(setEndpointVolume(0.3, deviceId), true);
    assert.ok(close(getEndpointVolume(deviceId).volume, 0.3));

    assert.equal(setEndpointMute(true, deviceId), true);
    assert.equal(getEndpointVolume(deviceId).muted, true);
    assert.equal(setEndpointMute(false, deviceId), true);
    assert.equal(getEndpointVolume(deviceId).muted, false);

    assert.equal(setEndpointVolumeDb(before.range.minDb, deviceId), true);
    assert.ok(close(getEndpointVolume(deviceId).volumeDb, before.range.minDb, 0.5));

    const { step } = getEndpointVolume(deviceId);
    assert.equal(stepEndpointVolume('up', deviceId), true);
    if (step.current < step.count - 1) {
      assert.ok(getEndpointVolume(deviceId).step.current > step.current);
    }
    assert.equal(stepEndpointVolume('down', deviceId), true);

    if (before.channels.length > 0) {
      assert.equal(setEndpointChannelVolume(0, 0.2, deviceId), true);
      assert.ok(close(getEndpointVolume(deviceId).channels[0].volume, 0.2));
    }
  } finally {
    setEndpointVolume(before.volume, deviceId);
    before.channels.forEach((ch, i) => setEndpointChannelVolume(i, ch.volume, deviceId));
    setEndpointMute(before.muted, deviceId);
  }
});

const assertPeak = (p) => {
  assert.equal(typeof p, 'number');
  assert.ok(p >= 0 && p <= 1, `bad peak ${p}`);
};

test('getEndpointPeak returns a 0..1 number or null', () => {
  const peak = getEndpointPeak();
  if (peak === null) assert.equal(getDefaultDevice(), null);
  else assertPeak(peak);
  const mic = getDefaultDevice('capture');
  if (mic !== null) assertPeak(getEndpointPeak(mic.id));
});

test('getEndpointPeak returns null for unknown ids', () => {
  assert.equal(getEndpointPeak('{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}'), null);
  assert.equal(getEndpointPeak(BAD_ID), null);
  assert.equal(getEndpointPeak(''), null);
});

test('getEndpointPeak never throws for devices in any state', () => {
  for (const device of listDevices({ state: STATES })) {
    const peak = getEndpointPeak(device.id);
    if (peak !== null) assertPeak(peak);
  }
});

test('getSessionPeak rejects malformed targets and matches nothing cleanly', () => {
  for (const target of BAD_TARGETS) assert.throws(() => getSessionPeak(target), /exactly one/);
  for (const target of UNMATCHED_TARGETS) assert.deepEqual(getSessionPeak(target), []);
});

test('getSessionPeak returns one 0..1 peak per listed session', () => {
  for (const s of listSessions({ includeSystemSounds: true })) {
    const result = getSessionPeak({ instanceId: s.instanceId });
    assert.ok(result.length <= 1);
    result.forEach(assertPeak);
  }
});

const ROLES = ['console', 'multimedia', 'communications'];

test('setDefaultDevice returns false for unknown ids', () => {
  assert.equal(setDefaultDevice('{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}'), false);
  assert.equal(setDefaultDevice(BAD_ID), false);
  assert.equal(setDefaultDevice(''), false);
});

test('setDefaultDevice rejects bad roles', () => {
  assert.throws(() => setDefaultDevice(BAD_ID, []), /roles must not be empty/);
  assert.throws(() => setDefaultDevice(BAD_ID, ['sideways']));
});

// Opt-in: switches the real default device. Set JSCAW_TEST_DEVICE to an active device id.
test('setDefaultDevice switches every role and can be restored', (t) => {
  const deviceId = process.env.JSCAW_TEST_DEVICE;
  const device = deviceId && getDevice(deviceId);
  if (!device || device.state !== 'active') {
    t.skip('set JSCAW_TEST_DEVICE to an active device id to run this check');
    return;
  }
  const before = ROLES.map((role) => getDefaultDevice(device.flow, role));
  try {
    assert.equal(setDefaultDevice(deviceId), true);
    for (const role of ROLES) assert.equal(getDefaultDevice(device.flow, role)?.id, deviceId, role);
    assert.equal(setDefaultDevice(deviceId, ['communications']), true);
  } finally {
    ROLES.forEach((role, i) => before[i] && setDefaultDevice(before[i].id, [role]));
  }
  ROLES.forEach((role, i) => assert.equal(getDefaultDevice(device.flow, role)?.id, before[i]?.id, role));
});

const ADDON_URL = new URL('../index.js', import.meta.url).href;

/** Runs `body` in a child process with the addon imported as `m`. */
function child(body, timeout = 10_000) {
  const src = `const m = await import(${JSON.stringify(ADDON_URL)});
${body}`;
  return spawnSync(process.execPath, ['--input-type=module', '-e', src], { timeout, encoding: 'utf8' });
}

test('onDeviceEvent returns an idempotent unsubscribe', () => {
  const unsubscribe = onDeviceEvent(() => {});
  assert.equal(typeof unsubscribe, 'function');
  unsubscribe();
  unsubscribe();
});

test('onDeviceEvent rejects a non-function callback', () => {
  assert.throws(() => onDeviceEvent(42));
});

test('a process exits cleanly while still subscribed', () => {
  const r = child('m.onDeviceEvent(() => {}); setTimeout(() => process.exit(0), 200);');
  assert.equal(r.status, 0, r.stderr);
});

test('unsubscribing lets the event loop drain', () => {
  const r = child('const u = m.onDeviceEvent(() => {}); setTimeout(u, 100);', 5000);
  assert.equal(r.status, 0, r.stderr);
});

test('a live subscription keeps the event loop alive', () => {
  const r = child('m.onDeviceEvent(() => {});', 1000);
  assert.equal(r.signal, 'SIGTERM', `exited early: ${r.status} ${r.stderr}`);
});

// Opt-in: switches the real console default. Needs JSCAW_TEST_DEVICE plus a second active
// device of the same flow to switch away from first.
test('onDeviceEvent reports default device changes', async (t) => {
  const deviceId = process.env.JSCAW_TEST_DEVICE;
  const device = deviceId && getDevice(deviceId);
  const other = device && listDevices({ flow: device.flow }).find((d) => d.id !== deviceId);
  if (!device || device.state !== 'active' || !other) {
    t.skip('set JSCAW_TEST_DEVICE to an active device id (with a second active device) to run this check');
    return;
  }
  const before = getDefaultDevice(device.flow, 'console');
  const events = [];
  const unsubscribe = onDeviceEvent((e) => events.push(e));
  try {
    setDefaultDevice(other.id, ['console']);
    setDefaultDevice(deviceId, ['console']);
    const matches = (e) =>
      e.type === 'defaultChanged' && e.role === 'console' && e.flow === device.flow && e.deviceId === deviceId;
    for (let i = 0; i < 30 && !events.some(matches); i++) await new Promise((r) => setTimeout(r, 100));
    assert.ok(events.some(matches), JSON.stringify(events));
  } finally {
    unsubscribe();
    if (before) setDefaultDevice(before.id, ['console']);
  }
});

// Opt-in, like the test above: a burst of events must stop at the unsubscribe.
test('no device events are delivered after unsubscribe', async (t) => {
  const deviceId = process.env.JSCAW_TEST_DEVICE;
  const device = deviceId && getDevice(deviceId);
  const other = device && listDevices({ flow: device.flow }).find((d) => d.id !== deviceId);
  if (!device || device.state !== 'active' || !other) {
    t.skip('set JSCAW_TEST_DEVICE to an active device id (with a second active device) to run this check');
    return;
  }
  const before = ROLES.map((role) => getDefaultDevice(device.flow, role));
  let calls = 0;
  const unsubscribe = onDeviceEvent(() => {
    calls++;
    unsubscribe();
  });
  try {
    // Every role switch fires several events in one burst.
    setDefaultDevice(other.id);
    setDefaultDevice(deviceId);
    await new Promise((r) => setTimeout(r, 1000));
    assert.equal(calls, 1);
  } finally {
    unsubscribe();
    ROLES.forEach((role, i) => before[i] && setDefaultDevice(before[i].id, [role]));
  }
});
