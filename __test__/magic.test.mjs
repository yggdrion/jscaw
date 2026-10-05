import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import assert from 'node:assert/strict';
// Imported through the package's own name so the `exports` map is exercised too.
import { listSessions, setSessionMute, setSessionVolume } from 'jscaw';
import { watchApp } from 'jscaw/magic';

test('watchApp on an absent app is inert and disposes cleanly', () => {
  for (const exeNames of ['jscaw-no-such-app.exe', ['jscaw-no-such-app.exe', 'nor-this.exe']]) {
    const app = watchApp(exeNames, { onVolume: assert.fail, onMute: assert.fail });
    assert.deepEqual(app.sessions, []);
    assert.equal(app.volume, null);
    assert.equal(app.mute, null);
    assert.equal(app.state, 'inactive');
    app.volume = 0.5;
    app.mute = true;
    app.toggleMute();
    app.stepVolume();
    assert.equal(app.volume, null);
    assert.equal(app.mute, null);
    app.dispose();
    app.dispose();
  }
});

test('watchApp rejects missing exe names', () => {
  assert.throws(() => watchApp([]), TypeError);
  assert.throws(() => watchApp(''), TypeError);
});

// Set JSCAW_TEST_PROCESS to the executable name of a currently-playing app to run it.
test('watchApp aggregates and fans out volume/mute on a real app', async (t) => {
  const targetProcess = process.env.JSCAW_TEST_PROCESS;
  if (!targetProcess) {
    t.skip('set JSCAW_TEST_PROCESS to a running process name to run this check');
    return;
  }
  const before = listSessions().filter(
    (s) => s.processName.toLowerCase() === targetProcess.toLowerCase()
  );
  assert.ok(before.length > 0, `no active audio session found for ${targetProcess}`);

  const volumeEvents = [];
  const app = watchApp(targetProcess.toUpperCase(), { onVolume: (v) => volumeEvents.push(v) });
  try {
    assert.equal(app.sessions.length, before.length);
    assert.equal(app.volume, Math.max(...before.map((s) => s.volume)));
    assert.equal(app.mute, before.some((s) => s.muted));

    app.volume = 0.3;
    assert.ok(Math.abs(app.volume - 0.3) < 0.01);
    app.stepVolume(0.2);
    assert.ok(Math.abs(app.volume - 0.5) < 0.01);
    app.stepVolume(-2);
    assert.ok(app.volume < 0.01);
    const muted = app.mute;
    app.toggleMute();
    assert.equal(app.mute, !muted);
    const ids = new Set(app.sessions.map((s) => s.instanceId));
    for (const s of listSessions().filter((s) => ids.has(s.instanceId))) {
      assert.ok(Math.abs(s.volume) < 0.01);
      assert.equal(s.muted, !muted);
    }

    await new Promise((resolve) => setTimeout(resolve, 200)); // let the native events arrive
    assert.deepEqual(volumeEvents, [], 'our own changes must not fire onVolume');

    // jscaw's event context is per-process, so a child process counts as an external change.
    const external = `require('jscaw').setSessionVolume({ processName: ${JSON.stringify(targetProcess)} }, 0.4)`;
    assert.equal(spawnSync(process.execPath, ['-e', external], { cwd: import.meta.dirname }).status, 0);
    await new Promise((resolve) => setTimeout(resolve, 200));
    assert.ok(volumeEvents.length > 0 && Math.abs(volumeEvents.at(-1) - 0.4) < 0.01, `got ${volumeEvents}`);
    assert.ok(Math.abs(app.volume - 0.4) < 0.01);
  } finally {
    app.dispose();
    for (const s of before) {
      const target = { instanceId: s.instanceId };
      setSessionVolume(target, s.volume);
      setSessionMute(target, s.muted);
    }
  }
});
