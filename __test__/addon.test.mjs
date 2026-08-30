import { test } from 'node:test';
import assert from 'node:assert/strict';
import { listSessions, setProcessVolume, setProcessMute } from '../index.js';

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
