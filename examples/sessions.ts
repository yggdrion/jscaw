import { getDefaultDevice, listSessions } from 'jscaw';

const mic = getDefaultDevice('capture');
for (const [label, deviceId] of [['speakers', undefined], ['microphone', mic?.id]] as const) {
  if (label === 'microphone' && !deviceId) continue;
  console.log(`${label}:`);
  for (const s of listSessions({ deviceId, includeSystemSounds: true })) {
    const name = s.isSystemSounds ? 'System sounds' : s.displayName || s.processName;
    console.log(
      `  ${name} (pid ${s.pid}, ${s.state}): ${Math.round(s.volume * 100)}%` +
        `${s.muted ? ' [muted]' : ''} instance=${s.instanceId}`,
    );
  }
}
// Mutating calls, all returning the number of sessions changed:
// setSessionVolume({ processName: 'Discord.exe' }, 0.3); setSessionMute({ pid: 1234 }, true);
// setSessionDisplayName({ instanceId }, 'Game'); setSessionGroupingParam({ instanceId }, '{…}');
// setSessionDuckingPreference({ processName: 'Spotify.exe' }, true);
