import { getDefaultDevice, listDevices } from 'jscaw';

for (const device of listDevices({ state: ['active', 'unplugged'] })) {
  console.log(`${device.flow.padEnd(7)} ${device.state.padEnd(9)} ${device.name}  ${device.id}`);
}

console.log('default speakers:', getDefaultDevice('render')?.name ?? '(none)');
console.log('default microphone:', getDefaultDevice('capture')?.name ?? '(none)');
