import { getEndpointPeak, getSessionPeak, listSessions } from 'jscaw';

const bar = (peak: number | null | undefined) =>
  peak == null ? '(n/a)' : `${'#'.repeat(Math.round(peak * 30)).padEnd(30)} ${peak.toFixed(2)}`;

// Read-only: samples the default speakers and every session 10 times, 100 ms apart.
for (let i = 0; i < 10; i++) {
  console.log(`speakers  ${bar(getEndpointPeak())}`);
  for (const s of listSessions()) {
    const [peak] = getSessionPeak({ instanceId: s.instanceId });
    console.log(`  ${(s.displayName || s.processName).padEnd(20)} ${bar(peak)}`);
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
}
