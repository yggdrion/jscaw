import { getDefaultDevice, listDevices, setDefaultDevice } from 'jscaw';

// Pass a device id to make it the default for every role; with no argument this is read-only.
const target = process.argv[2];
if (target) {
  console.log(setDefaultDevice(target) ? 'switched' : `no device with id ${target}`);
}

const current = getDefaultDevice('render')?.id;
for (const device of listDevices({ flow: 'render' })) {
  console.log(`${device.id === current ? '*' : ' '} ${device.name}  ${device.id}`);
}
