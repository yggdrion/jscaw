import { onEndpointVolumeChange } from 'jscaw';

// Drag the Windows volume slider or press the media volume keys to see events.
const unsubscribe = onEndpointVolumeChange((event) => console.log(new Date().toISOString(), event));
if (!unsubscribe) {
  console.log('no default render device');
  process.exit(0);
}
console.log('listening for endpoint volume changes, Ctrl+C to stop');

process.on('SIGINT', () => {
  unsubscribe(); // releases the subscription, so the process can exit on its own
});
