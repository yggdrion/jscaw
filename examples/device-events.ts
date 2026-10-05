import { onDeviceEvent } from 'jscaw';

// Plug in headphones, disable a device in Sound settings or switch the default to see events.
const unsubscribe = onDeviceEvent((event) => console.log(new Date().toISOString(), event));
console.log('listening for device events, Ctrl+C to stop');

process.on('SIGINT', () => {
  unsubscribe(); // releases the subscription, so the process can exit on its own
});
