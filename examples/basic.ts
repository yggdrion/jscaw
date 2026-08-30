import { listSessions, setProcessMute, setProcessVolume } from '@r4b2/win-audio-sessions';

for (const session of listSessions()) {
  console.log(`${session.processName} (pid ${session.pid}): volume=${session.volume} muted=${session.muted}`);
}

const updated = setProcessVolume('Discord.exe', 0.3);
console.log(`set volume on ${updated} Discord.exe session(s)`);

setProcessMute('Discord.exe', false);
