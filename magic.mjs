// jscaw/magic: pycaw's magic.py. Hand-written (not napi-generated) on top of the native API.
import {
  listSessions,
  onSessionCreated,
  onSessionEvent,
  setSessionMute,
  setSessionVolume,
} from './index.js';

/**
 * Watches every default-device session of the given executables (case-insensitive) as they
 * come and go, and controls them as one. Callbacks fire when the aggregate changes, and only
 * for changes made outside jscaw unless `includeSelf` is set.
 */
export function watchApp(exeNames, options = {}) {
  const names = new Set([exeNames].flat().map((n) => n.toLowerCase()));
  if (names.size === 0 || names.has('')) throw new TypeError('watchApp needs at least one exe name');
  const { onVolume, onMute, onState, onSessionsChanged, includeSelf = false } = options;

  const tracked = new Map(); // instanceId -> { session, unsubscribe }
  let seeding = true; // the initial sessions are the starting state, not a change to report
  const sessions = () => [...tracked.values()].map((t) => t.session);
  const aggregate = () => {
    const all = sessions();
    return {
      volume: all.length ? Math.max(...all.map((s) => s.volume)) : null,
      mute: all.length ? all.some((s) => s.muted) : null,
      state: all.some((s) => s.state === 'active') ? 'active' : 'inactive',
    };
  };
  // Runs `change`, then fires the callbacks whose aggregate value it moved.
  const notify = (change, external = true) => {
    const before = aggregate();
    change();
    const after = aggregate();
    if (seeding) return;
    if (external || includeSelf) {
      if (after.volume !== null && after.volume !== before.volume) onVolume?.(after.volume);
      if (after.mute !== null && after.mute !== before.mute) onMute?.(after.mute);
    }
    if (after.state !== before.state) onState?.(after.state);
  };

  const untrack = (instanceId) => {
    const entry = tracked.get(instanceId);
    if (!entry) return;
    notify(() => tracked.delete(instanceId));
    entry.unsubscribe();
    onSessionsChanged?.(sessions());
  };

  const track = (session) => {
    if (!names.has(session.processName.toLowerCase()) || tracked.has(session.instanceId)) return;
    const { instanceId } = session;
    const unsubscribe = onSessionEvent({ instanceId }, (event) => {
      const entry = tracked.get(instanceId);
      if (!entry) return;
      if (event.type === 'disconnected' || event.state === 'expired') return untrack(instanceId);
      if (event.type === 'volumeChanged') {
        const { volume, muted } = event;
        notify(() => (entry.session = { ...entry.session, volume, muted }), !event.selfInitiated);
      } else if (event.type === 'stateChanged') {
        notify(() => (entry.session = { ...entry.session, state: event.state }));
      }
    });
    if (!unsubscribe) return; // expired between listing and subscribing
    notify(() => tracked.set(instanceId, { session, unsubscribe }));
    if (!seeding) onSessionsChanged?.(sessions());
  };

  // Subscribe before listing so a session created in between isn't missed; `track` dedupes.
  // `null` means there's no default render device: the app just stays empty.
  let stopCreated = onSessionCreated(track);
  listSessions().forEach(track);
  seeding = false;

  // Our own writes: update the cache now so getters agree immediately. A session that just
  // expired matches nothing (0), so its cache stays as is until its expiry event untracks it.
  const fanOut = (setter, value, patch) => {
    for (const entry of tracked.values()) {
      if (setter({ instanceId: entry.session.instanceId }, value) > 0) {
        notify(() => (entry.session = { ...entry.session, ...patch }), false);
      }
    }
  };

  return {
    get sessions() {
      return sessions();
    },
    get volume() {
      return aggregate().volume;
    },
    set volume(volume) {
      fanOut(setSessionVolume, volume, { volume });
    },
    get mute() {
      return aggregate().mute;
    },
    set mute(muted) {
      fanOut(setSessionMute, muted, { muted });
    },
    get state() {
      return aggregate().state;
    },
    toggleMute() {
      const { mute } = aggregate();
      if (mute !== null) this.mute = !mute;
    },
    stepVolume(step = 0.1) {
      const { volume } = aggregate();
      if (volume !== null) this.volume = Math.min(1, Math.max(0, volume + step));
    },
    dispose() {
      stopCreated?.();
      stopCreated = null;
      for (const { unsubscribe } of tracked.values()) unsubscribe();
      tracked.clear();
    },
  };
}
