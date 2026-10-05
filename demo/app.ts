import type { AudioSession, Device, DeviceRole, EndpointVolume } from '../index.js';

const $ = (id: string) => document.getElementById(id)!;

// Tiny element builder: h('button', { onclick }, 'text', child…)
function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<Record<string, unknown>> = {},
  ...children: (Node | string | null | false)[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  Object.assign(el, props);
  for (const child of children) if (child !== null && child !== false) el.append(child);
  return el;
}

// --- RPC over the WebSocket -------------------------------------------------------------
const ws = new WebSocket(`ws://${location.host}/ws`);
const pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();
let nextId = 1;

function call<T = any>(fn: string, ...args: unknown[]): Promise<T> {
  const id = nextId++;
  ws.send(JSON.stringify({ id, fn, args }));
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}

// Runs a call and logs failures instead of throwing, for UI handlers.
const act = (fn: string, ...args: unknown[]) =>
  call(fn, ...args).catch((e) => log(`error: ${fn}`, e.message));

function log(kind: string, data: unknown, target = $('event-log')) {
  const line = h('div', {}, `${new Date().toLocaleTimeString()} ${kind} ${JSON.stringify(data)}`);
  target.prepend(line);
  while (target.childElementCount > 200) target.lastElementChild!.remove();
}

function slider(value: number, oninput: (v: number) => void) {
  return h('input', {
    type: 'range',
    min: 0,
    max: 1,
    step: 0.01,
    value,
    oninput: (e: Event) => oninput(Number((e.target as HTMLInputElement).value)),
  });
}

function checkbox(checked: boolean, onchange: (v: boolean) => void, label: string) {
  const box = h('input', { type: 'checkbox', checked, onchange: () => onchange(box.checked) });
  return h('label', {}, box, ` ${label}`);
}

function textField(value: string, onchange: (v: string) => void, placeholder: string) {
  const input = h('input', { type: 'text', value, placeholder, onchange: () => onchange(input.value) });
  return input;
}

// --- Devices ------------------------------------------------------------------------------
const roles: DeviceRole[] = ['console', 'multimedia', 'communications'];

async function renderDevices() {
  const state = ($('device-state') as HTMLSelectElement).value.split(',');
  const devices = await call<Device[]>('listDevices', { flow: 'all', state });
  const defaults = new Map<string, string[]>();
  for (const flow of ['render', 'capture'])
    for (const role of roles) {
      const d = await call<Device | null>('getDefaultDevice', flow, role);
      if (d) defaults.set(d.id, [...(defaults.get(d.id) ?? []), role]);
    }

  const list = $('device-list');
  list.replaceChildren(
    ...devices.map((d) => {
      const props = h('table');
      const details = h('details', {}, h('summary', { className: 'muted' }, 'properties'), props);
      details.addEventListener('toggle', async () => {
        if (!details.open || props.childElementCount) return;
        const record = (await call('getDeviceProperties', d.id)) ?? {};
        props.append(...Object.entries(record).map(([k, v]) => h('tr', {}, h('td', {}, k), h('td', {}, String(v)))));
      });
      return h(
        'div',
        { className: 'item' },
        h(
          'div',
          { className: 'row' },
          h('strong', {}, d.name),
          h('span', { className: 'muted' }, `${d.flow} · ${d.state}`),
          ...(defaults.get(d.id) ?? []).map((r) => h('span', { className: 'tag' }, r)),
          d.state === 'active' &&
            h('button', { onclick: () => act('setDefaultDevice', d.id).then(renderDevices) }, 'Make default'),
        ),
        details,
      );
    }),
  );
  if (!devices.length) list.append(h('p', { className: 'muted' }, 'No devices.'));
  await renderEndpointDevices();
}

// --- Endpoint volume ---------------------------------------------------------------------
let endpointDevice: string | null = null;

async function renderEndpointDevices() {
  const devices = await call<Device[]>('listDevices', { flow: 'all' });
  const select = $('endpoint-device') as HTMLSelectElement;
  select.replaceChildren(
    h('option', { value: '' }, 'default render device'),
    ...devices.map((d) => h('option', { value: d.id }, `${d.name} (${d.flow})`)),
  );
  select.value = endpointDevice ?? '';
}

async function renderEndpoint(external = false) {
  const v = await call<EndpointVolume | null>('getEndpointVolume', endpointDevice);
  const body = $('endpoint-body');
  if (!v) return body.replaceChildren(h('p', { className: 'muted' }, 'No device.'));
  const id = endpointDevice;
  const db = h('input', {
    type: 'number',
    min: v.range.minDb,
    max: v.range.maxDb,
    step: v.range.incrementDb,
    value: v.volumeDb.toFixed(1),
    onchange: () => act('setEndpointVolumeDb', Number(db.value), id),
  });
  body.replaceChildren(
    h(
      'div',
      { className: 'row' },
      'Master',
      slider(v.volume, (x) => act('setEndpointVolume', x, id)),
      db,
      'dB',
      checkbox(v.muted, (m) => act('setEndpointMute', m, id), 'mute'),
      h('button', { onclick: () => act('stepEndpointVolume', 'down', id).then(() => renderEndpoint()) }, '−'),
      h('button', { onclick: () => act('stepEndpointVolume', 'up', id).then(() => renderEndpoint()) }, '+'),
      external && h('span', { className: 'external' }, 'changed externally'),
    ),
    ...v.channels.map((c, i) =>
      h('div', { className: 'row' }, `Channel ${i}`, slider(c.volume, (x) => act('setEndpointChannelVolume', i, x, id))),
    ),
    h(
      'div',
      { className: 'muted' },
      `range ${v.range.minDb}…${v.range.maxDb} dB, step ${v.step.current}/${v.step.count}, hardware ${v.hardwareSupport}`,
    ),
  );
}

$('endpoint-device').addEventListener('change', async (e) => {
  endpointDevice = (e.target as HTMLSelectElement).value || null;
  await call('watchEndpoint', endpointDevice);
  await renderEndpoint();
});

// --- Sessions ------------------------------------------------------------------------------
async function renderSessions() {
  const includeSystemSounds = ($('system-sounds') as HTMLInputElement).checked;
  const sessions = await call<AudioSession[]>('listSessions', { includeSystemSounds });
  const list = $('session-list');
  list.replaceChildren(
    ...(await Promise.all(
      sessions.map(async (s) => {
        const target = { instanceId: s.instanceId };
        const [channels = []] = await call<number[][]>('getSessionChannelVolumes', target);
        return h(
          'div',
          { className: 'item' },
          h(
            'div',
            { className: 'row' },
            h('strong', {}, s.displayName || s.processName || 'System sounds'),
            h('span', { className: 'muted' }, `pid ${s.pid} · ${s.state}`),
          ),
          h(
            'div',
            { className: 'row' },
            slider(s.volume, (x) => act('setSessionVolume', target, x)),
            checkbox(s.muted, (m) => act('setSessionMute', target, m), 'mute'),
            h('meter', { min: 0, max: 1, value: 0 }),
            checkbox(false, (o) => act('setSessionDuckingPreference', target, o), 'no ducking'),
          ),
          ...channels.map((c, i) =>
            h('div', { className: 'row' }, `Channel ${i}`, slider(c, (x) => act('setSessionChannelVolume', target, i, x))),
          ),
          h(
            'div',
            { className: 'row' },
            textField(s.displayName, (x) => act('setSessionDisplayName', target, x), 'display name'),
            textField(s.iconPath, (x) => act('setSessionIconPath', target, x), 'icon path'),
            textField(s.groupingParam, (x) => act('setSessionGroupingParam', target, x), 'grouping GUID'),
          ),
        );
      }),
    )),
  );
  // dataset can't be set through Object.assign, so tag the meters afterwards
  list.querySelectorAll('meter').forEach((m, i) => (m.dataset.instance = sessions[i].instanceId));
  if (!sessions.length) list.append(h('p', { className: 'muted' }, 'No sessions.'));
}

$('system-sounds').addEventListener('change', renderSessions);

// --- Magic ---------------------------------------------------------------------------------
const magicRefresh = new Map<number, () => void>();

$('magic-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const exe = ($('magic-exe') as HTMLInputElement).value.trim();
  const id = await call<number>('watchApp', exe);
  const state = h('span', { className: 'muted' });
  const callbacks = h('div', { className: 'log' });
  const card = h('div', { className: 'item', id: `magic-${id}` });
  const op = async (name: string, value?: unknown) => {
    const s = await call('magic', id, name, value).catch((err) => (log('error: magic', err.message), null));
    if (s) state.textContent = `volume ${s.volume?.toFixed(2) ?? '–'} · mute ${s.mute ?? '–'} · ${s.state} · ${s.sessions.length} session(s)`;
  };
  card.append(
    h(
      'div',
      { className: 'row' },
      h('strong', {}, exe),
      state,
      h('button', { onclick: () => op('stepVolume', -0.1) }, '−'),
      h('button', { onclick: () => op('stepVolume', 0.1) }, '+'),
      h('button', { onclick: () => op('toggleMute') }, 'toggle mute'),
      h('button', { onclick: () => call('magic', id, 'dispose').then(() => (magicRefresh.delete(id), card.remove())) }, 'dispose'),
    ),
    callbacks,
  );
  $('magic-list').prepend(card);
  magicRefresh.set(id, () => op('get'));
  op('get');
});

// --- Events --------------------------------------------------------------------------------
const debounce = (fn: () => unknown, ms = 150) => {
  let t: ReturnType<typeof setTimeout>;
  return () => (clearTimeout(t), (t = setTimeout(fn, ms)));
};
const refreshDevices = debounce(renderDevices);
const refreshSessions = debounce(renderSessions);

ws.addEventListener('open', () => {
  $('status').textContent = 'connected';
  renderDevices();
  renderEndpoint();
  renderSessions();
});
ws.addEventListener('close', () => ($('status').textContent = 'disconnected'));

ws.addEventListener('message', (e) => {
  const msg = JSON.parse(e.data);
  if ('id' in msg) {
    const p = pending.get(msg.id);
    pending.delete(msg.id);
    return 'error' in msg ? p?.reject(new Error(msg.error)) : p?.resolve(msg.result);
  }
  if (msg.type === 'meters') {
    ($('endpoint-peak') as HTMLMeterElement).value = msg.endpoint ?? 0;
    for (const m of document.querySelectorAll<HTMLMeterElement>('#session-list meter'))
      m.value = msg.sessions[m.dataset.instance!] ?? 0;
    return;
  }
  if (msg.type === 'magic') {
    log(msg.kind, msg.value, $(`magic-${msg.id}`)?.querySelector('.log') ?? $('event-log'));
    magicRefresh.get(msg.id)?.();
    return;
  }
  log(msg.type, msg.event);
  if (msg.type === 'duck') log(msg.event.type, msg.event, $('duck-log'));
  if (msg.type === 'device') {
    refreshDevices();
    if (msg.event.type === 'defaultChanged' && !endpointDevice) call('watchEndpoint', null).then(() => renderEndpoint());
  }
  // Own changes are already shown by the control that made them; only re-read external ones.
  if (msg.type === 'endpointVolume' && !msg.event.selfInitiated) renderEndpoint(true);
  if (msg.type === 'sessionCreated') refreshSessions();
  if (msg.type === 'session' && !msg.event.selfInitiated) refreshSessions();
});
