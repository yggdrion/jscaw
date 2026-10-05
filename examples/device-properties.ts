import { getDeviceProperties, listDevices } from 'jscaw';

for (const device of listDevices()) {
  console.log(`${device.name} (${device.flow})`);
  for (const [key, value] of Object.entries(getDeviceProperties(device.id) ?? {})) {
    console.log(`  ${key.padEnd(44)} ${JSON.stringify(value)}`);
  }
}
