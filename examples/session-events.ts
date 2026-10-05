import { onDuckEvent, onSessionCreated, onSessionEvent } from 'jscaw';

// Start playing audio in an app, drag its slider in the Windows volume mixer, or join a call
// in Teams/Discord to see events.
const exeName = process.argv[2] ?? 'firefox.exe';
const log = (kind: string) => (event: unknown) => console.log(new Date().toISOString(), kind, event);

const unsubscribers = [
  onSessionCreated(log('created')),
  onSessionEvent({ processName: exeName }, log(exeName)),
  onDuckEvent(log('duck')),
].filter((u) => u !== null);
if (unsubscribers.length === 0) {
  console.log('no default render device');
  process.exit(0);
}
console.log(`listening for session events (watching ${exeName}), Ctrl+C to stop`);

process.on('SIGINT', () => {
  unsubscribers.forEach((unsubscribe) => unsubscribe()); // lets the process exit on its own
});
