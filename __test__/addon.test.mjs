import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  getDefaultDevice,
  getDevice,
  getEndpointVolume,
  listDevices,
  listSessions,
  setProcessMute,
  setProcessVolume,
} from '../index.js';

test('listSessions returns an array of session-shaped objects', () => {
  const sessions = listSessions();
  assert.ok(Array.isArray(sessions));
  for (const session of sessions) {
    assert.equal(typeof session.pid, 'number');
    assert.equal(typeof session.processName, 'string');
    assert.equal(typeof session.volume, 'number');
    assert.equal(typeof session.muted, 'boolean');
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

const FLOWS = ['render', 'capture'];
const STATES = ['active', 'disabled', 'notPresent', 'unplugged'];
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
