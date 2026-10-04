import { getDefaultDevice, getEndpointVolume } from 'jscaw';

for (const flow of ['render', 'capture'] as const) {
  const device = getDefaultDevice(flow);
  const ev = device && getEndpointVolume(device.id);
  if (!device || !ev) {
    console.log(`${flow}: (no device)`);
    continue;
  }
  console.log(
    `${flow}: ${device.name} — ${Math.round(ev.volume * 100)}% (${ev.volumeDb.toFixed(1)} dB)` +
      `${ev.muted ? ' [muted]' : ''}, ${ev.channels.length} channel(s), ` +
      `range ${ev.range.minDb}..${ev.range.maxDb} dB`,
  );
}
// Mutating calls, all returning false when the device is missing:
// setEndpointVolume(0.5); setEndpointVolumeDb(-20); setEndpointMute(true);
// setEndpointChannelVolume(0, 0.5); stepEndpointVolume('up');
