// Local demo UI for every jscaw API: `bun demo/server.ts` (or `pnpm demo`), then open the URL.
import type { Server } from 'bun';
import * as jscaw from '../index.js';
import { watchApp, type MagicApp } from '../magic.mjs';
import index from './index.html';

type Unsubscribe = () => void;

let server: Server<undefined>;
let meterTimer: ReturnType<typeof setInterval> | undefined;
let stopEndpoint: Unsubscribe | null = null;
const globalSubs: Unsubscribe[] = [];
const sessionSubs = new Map<string, Unsubscribe>();
const magicApps = new Map<number, MagicApp>();
let nextMagicId = 1;

const publish = (type: string, data: object) =>
  server.publish('events', JSON.stringify({ type, ...data }));

function trackSession(session: jscaw.AudioSession) {
  const { instanceId } = session;
  if (sessionSubs.has(instanceId)) return;
  const unsubscribe = jscaw.onSessionEvent({ instanceId }, (event) => {
    publish('session', { event });
    if (event.type === 'disconnected' || (event.type === 'stateChanged' && event.state === 'expired')) {
      sessionSubs.get(instanceId)?.();
      sessionSubs.delete(instanceId);
    }
  });
  if (unsubscribe) sessionSubs.set(instanceId, unsubscribe);
}

// Demo-only RPCs next to the jscaw allow-list: magic watchers live server-side, by id.
const demoFns: Record<string, (...args: any[]) => unknown> = {
  watchApp(exeName: string) {
    const id = nextMagicId++;
    const emit = (kind: string) => (value: unknown) => publish('magic', { id, kind, value });
    magicApps.set(
      id,
      watchApp(exeName, {
        onVolume: emit('volume'),
        onMute: emit('mute'),
        onState: emit('state'),
        onSessionsChanged: emit('sessions'),
      }),
    );
    return id;
  },
  magic(id: number, op: string, value?: unknown) {
    const app = magicApps.get(id);
    if (!app) throw new Error(`unknown magic id: ${id}`);
    if (op === 'volume') app.volume = value as number;
    else if (op === 'mute') app.mute = value as boolean;
    else if (op === 'toggleMute') app.toggleMute();
    else if (op === 'stepVolume') app.stepVolume(value as number | undefined);
    else if (op === 'dispose') {
      app.dispose();
      magicApps.delete(id);
      return null;
    } else if (op !== 'get') throw new Error(`unknown magic op: ${op}`);
    return { volume: app.volume, mute: app.mute, state: app.state, sessions: app.sessions };
  },
  watchEndpoint(deviceId?: string) {
    stopEndpoint?.();
    stopEndpoint = jscaw.onEndpointVolumeChange((event) => publish('endpointVolume', { event }), deviceId);
    return stopEndpoint !== null;
  },
};

// Every getter/setter jscaw exports is callable; `on*` subscriptions stay server-side.
const rpc: Record<string, (...args: any[]) => unknown> = { ...demoFns };
for (const [name, value] of Object.entries(jscaw)) {
  if (typeof value === 'function' && !name.startsWith('on')) rpc[name] = value as any;
}

export function startDemo(port = 3000) {
  server = Bun.serve({
    hostname: '127.0.0.1',
    port,
    routes: { '/': index },
    fetch(req, srv) {
      if (new URL(req.url).pathname === '/ws' && srv.upgrade(req)) return;
      return new Response('not found', { status: 404 });
    },
    websocket: {
      open(ws) {
        ws.subscribe('events');
      },
      message(ws, raw) {
        const { id, fn, args = [] } = JSON.parse(String(raw));
        try {
          if (!Object.hasOwn(rpc, fn)) throw new Error(`unknown fn: ${fn}`);
          ws.send(JSON.stringify({ id, result: rpc[fn](...args) ?? null }));
        } catch (e) {
          ws.send(JSON.stringify({ id, error: (e as Error).message }));
        }
      },
    },
  });

  for (const unsubscribe of [
    jscaw.onDeviceEvent((event) => publish('device', { event })),
    jscaw.onSessionCreated((session) => {
      trackSession(session);
      publish('sessionCreated', { event: session });
    }),
    jscaw.onDuckEvent((event) => publish('duck', { event })),
  ]) {
    if (unsubscribe) globalSubs.push(unsubscribe); // null: no default device
  }
  demoFns.watchEndpoint();
  jscaw.listSessions({ includeSystemSounds: true }).forEach(trackSession);

  meterTimer = setInterval(() => {
    if (server.subscriberCount('events') === 0) return;
    const sessions: Record<string, number> = {};
    for (const instanceId of sessionSubs.keys()) {
      sessions[instanceId] = jscaw.getSessionPeak({ instanceId })[0] ?? 0;
    }
    publish('meters', { endpoint: jscaw.getEndpointPeak(), sessions });
  }, 50);

  return server;
}

export function stopDemo() {
  clearInterval(meterTimer);
  for (const app of magicApps.values()) app.dispose();
  magicApps.clear();
  for (const unsubscribe of [...globalSubs, ...sessionSubs.values()]) unsubscribe();
  globalSubs.length = 0;
  sessionSubs.clear();
  stopEndpoint?.();
  stopEndpoint = null;
  server.stop(true);
}

if (import.meta.main) {
  const srv = startDemo(Number(process.env.PORT ?? 3000));
  console.log(`jscaw demo on http://localhost:${srv.port} (Ctrl+C to stop)`);
  process.on('SIGINT', () => {
    stopDemo(); // releases every subscription, so the process exits on its own
  });
}
