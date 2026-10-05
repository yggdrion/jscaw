import { afterAll, expect, test } from 'bun:test';
import { startDemo, stopDemo } from './server.ts';

const server = startDemo(0);
afterAll(() => {
  ws.close();
  stopDemo();
});

let nextId = 1;
const ws = new WebSocket(`ws://127.0.0.1:${server.port}/ws`);
const opened = new Promise((resolve) => ws.addEventListener('open', resolve, { once: true }));

async function call(fn: string, ...args: unknown[]) {
  await opened;
  const id = nextId++;
  return new Promise<{ id: number; result?: unknown; error?: string }>((resolve) => {
    const onMessage = (e: MessageEvent) => {
      const msg = JSON.parse(e.data);
      if (msg.id !== id) return; // skip published events
      ws.removeEventListener('message', onMessage);
      resolve(msg);
    };
    ws.addEventListener('message', onMessage);
    ws.send(JSON.stringify({ id, fn, args }));
  });
}

test('GET / serves the HTML page', async () => {
  const res = await fetch(`http://127.0.0.1:${server.port}/`);
  expect(res.status).toBe(200);
  expect(res.headers.get('content-type')).toContain('text/html');
});

test('listDevices returns an array', async () => {
  expect(Array.isArray((await call('listDevices')).result)).toBe(true);
});

test('unknown fn returns an error', async () => {
  expect((await call('eval', '1')).error).toContain('unknown fn');
});

test('subscriptions are not callable over RPC', async () => {
  expect((await call('onDeviceEvent')).error).toContain('unknown fn');
});

test('jscaw errors come back as error messages', async () => {
  expect((await call('setEndpointVolume', 2)).error).toBeString();
});

