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
