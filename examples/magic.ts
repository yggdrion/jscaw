import { watchApp } from 'jscaw/magic';

// Drag the app's slider in the Windows volume mixer, or start/stop its audio, to see callbacks.
const exeName = process.argv[2] ?? 'firefox.exe';
const log = (kind: string) => (value: unknown) => console.log(new Date().toISOString(), kind, value);

const app = watchApp(exeName, {
  onVolume: log('volume'),
  onMute: log('mute'),
  onState: log('state'),
  onSessionsChanged: (sessions) => log('sessions')(sessions.length),
});
console.log(`${exeName}: ${app.sessions.length} session(s), volume ${app.volume}, mute ${app.mute}`);
console.log('watching, Ctrl+C to stop');

process.on('SIGINT', () => app.dispose()); // lets the process exit on its own
