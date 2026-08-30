#!/usr/bin/env node
// Pre-release check: builds the pushed tip of `main`, packs it into the same tarballs
// `npm publish` would produce, then installs those tarballs via `bun add` into a scratch
// consumer project and runs the test suite against the installed package — not the source
// tree. Catches packaging bugs (missing `files` entries, a platform binding that doesn't
// resolve) that in-repo tests can't see, since those import ../index.js directly.
// Run: pnpm smoke-test  (see docs/RELEASING.md and AGENTS.md).

import { execSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, cpSync, rmSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

function quote(arg) {
  return /^[\w./:@-]+$/.test(arg) ? arg : `"${arg.replace(/"/g, '\\"')}"`;
}

function run(cmd, args, opts = {}) {
  const line = [cmd, ...args.map(quote)].join(' ');
  console.log(`$ ${line}`);
  execSync(line, { stdio: 'inherit', ...opts });
}

function runCapture(cmd, args, opts = {}) {
  const line = [cmd, ...args.map(quote)].join(' ');
  return execSync(line, { encoding: 'utf8', ...opts }).trim();
}

const target = `win32-${process.arch}-msvc`;
if (!['win32-x64-msvc', 'win32-arm64-msvc'].includes(target)) {
  console.error(`Unsupported host arch for this addon: ${process.arch} (expected x64 or arm64)`);
  process.exit(1);
}

const repoRoot = process.cwd();
const scratch = mkdtempSync(path.join(tmpdir(), 'wads-smoketest-'));
const worktree = path.join(scratch, 'main');
const tarballDir = path.join(scratch, 'tarballs');
const consumer = path.join(scratch, 'consumer');

let step = 'setup';
try {
  step = 'fetch main';
  run('git', ['fetch', 'origin', 'main'], { cwd: repoRoot });

  step = 'add worktree';
  run('git', ['worktree', 'add', worktree, 'origin/main'], { cwd: repoRoot });
  const sha = runCapture('git', ['rev-parse', '--short', 'HEAD'], { cwd: worktree });
  console.log(`\nSmoke-testing origin/main @ ${sha}\n`);

  step = 'install';
  run('pnpm', ['install'], { cwd: worktree });

  step = 'build';
  run('pnpm', ['build', '--release'], { cwd: worktree });

  step = 'locate built binary';
  const builtBinary = path.join(worktree, `win-audio-sessions.${target}.node`);
  if (!existsSync(builtBinary)) {
    throw new Error(`Expected build output missing: ${builtBinary}`);
  }
  const platformDir = path.join(worktree, 'npm', target);
  cpSync(builtBinary, path.join(platformDir, path.basename(builtBinary)));

  step = 'pack tarballs';
  mkdirSync(tarballDir, { recursive: true });
  run('npm', ['pack', '--pack-destination', tarballDir], { cwd: worktree });
  run('npm', ['pack', '--pack-destination', tarballDir], { cwd: platformDir });

  step = 'locate tarballs';
  const rootPkg = JSON.parse(readFileSync(path.join(worktree, 'package.json'), 'utf8'));
  const platformPkg = JSON.parse(readFileSync(path.join(platformDir, 'package.json'), 'utf8'));
  const tarballName = (pkg) => `${pkg.name.replace('@', '').replace('/', '-')}-${pkg.version}.tgz`;
  const rootTarball = path.join(tarballDir, tarballName(rootPkg));
  const platformTarball = path.join(tarballDir, tarballName(platformPkg));
  for (const t of [rootTarball, platformTarball]) {
    if (!existsSync(t)) throw new Error(`Expected tarball missing: ${t}`);
  }

  step = 'install into scratch consumer';
  mkdirSync(consumer, { recursive: true });
  run('bun', ['add', rootTarball], { cwd: consumer });
  run('bun', ['add', platformTarball], { cwd: consumer });

  step = 'copy test suite';
  const testSrc = readFileSync(path.join(repoRoot, '__test__', 'addon.test.mjs'), 'utf8');
  const testForConsumer = testSrc.replace("from '../index.js'", `from '${rootPkg.name}'`);
  writeFileSync(path.join(consumer, 'addon.test.mjs'), testForConsumer);

  step = 'run tests under bun';
  run('bun', ['test', 'addon.test.mjs'], { cwd: consumer });

  step = 'run tests under node';
  run('node', ['--test', 'addon.test.mjs'], { cwd: consumer });

  step = 'copy example';
  const exampleSrc = readFileSync(path.join(repoRoot, 'examples', 'basic.ts'), 'utf8');
  // Never mutate a real audio session by default: gate the write calls behind the same
  // opt-in env var the test suite uses, instead of running against a hardcoded process.
  const exampleForConsumer = exampleSrc.replace(
    /\nconst updated = setProcessVolume[\s\S]*$/,
    `
const targetProcess = process.env.WIN_AUDIO_SESSIONS_TEST_PROCESS;
if (targetProcess) {
  const updated = setProcessVolume(targetProcess, 0.3);
  console.log(\`set volume on \${updated} \${targetProcess} session(s)\`);
  setProcessMute(targetProcess, false);
} else {
  console.log('WIN_AUDIO_SESSIONS_TEST_PROCESS not set — skipping volume/mute example calls');
}
`
  );
  writeFileSync(path.join(consumer, 'basic.ts'), exampleForConsumer);

  step = 'run example under bun';
  run('bun', ['run', 'basic.ts'], { cwd: consumer });

  console.log(`\nPASS — origin/main @ ${sha} installs and tests clean as a bun dependency (${target}).\n`);
} catch (err) {
  console.error(`\nFAIL at step "${step}": ${err.message}\n`);
  process.exitCode = 1;
} finally {
  try {
    if (existsSync(worktree)) {
      run('git', ['worktree', 'remove', '--force', worktree], { cwd: repoRoot });
    }
  } catch {
    // Windows can briefly hold a lock on freshly-built artifacts (AV scanning, etc.), which
    // makes git's own rmdir fail here even with --force. The rmSync below still removes the
    // directory; this just clears git's now-dangling worktree admin entry for it.
    try {
      run('git', ['worktree', 'prune'], { cwd: repoRoot });
    } catch {
      // best-effort
    }
  }
  rmSync(scratch, { recursive: true, force: true });
}
