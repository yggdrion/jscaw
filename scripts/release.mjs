#!/usr/bin/env node
// Cuts a release: bumps the version everywhere it needs to live in sync, commits, tags, and
// pushes. Run from a clean main: `pnpm release patch|minor|major`.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const bump = process.argv[2];
if (!['patch', 'minor', 'major'].includes(bump)) {
  console.error('Usage: pnpm release <patch|minor|major>');
  process.exit(1);
}

function run(cmd, args, opts = {}) {
  console.log(`$ ${cmd} ${args.join(' ')}`);
  return execFileSync(cmd, args, { stdio: 'inherit', ...opts });
}

function runCapture(cmd, args) {
  return execFileSync(cmd, args, { encoding: 'utf8' }).trim();
}

const branch = runCapture('git', ['rev-parse', '--abbrev-ref', 'HEAD']);
if (branch !== 'main') {
  console.error(`Must release from main, currently on "${branch}".`);
  process.exit(1);
}

const dirty = runCapture('git', ['status', '--porcelain']);
if (dirty) {
  console.error('Working tree is not clean:\n' + dirty);
  process.exit(1);
}

run('git', ['fetch', 'origin', 'main']);
run('git', ['pull', '--ff-only', 'origin', 'main']);

// Bump package.json only (no git actions) — npm computes the semver bump for us.
run('npm', ['version', bump, '--no-git-tag-version']);

const version = JSON.parse(readFileSync('package.json', 'utf8')).version;
const tag = `v${version}`;

// Sync the platform sub-packages' committed package.json version to match.
run('pnpm', ['--package=@napi-rs/cli', 'dlx', 'napi', 'version']);

// Keep Cargo.toml's version in step (not published anywhere, but should stay honest).
const cargoToml = readFileSync('Cargo.toml', 'utf8');
writeFileSync('Cargo.toml', cargoToml.replace(/^version = ".*"$/m, `version = "${version}"`));

// cargo check (not build) is enough to refresh our own crate's entry in Cargo.lock.
run('cargo', ['check', '--release']);

run('git', [
  'add',
  'package.json',
  'Cargo.toml',
  'Cargo.lock',
  'npm/win32-x64-msvc/package.json',
  'npm/win32-arm64-msvc/package.json',
]);
run('git', ['commit', '-m', `chore(release): ${tag}`]);
run('git', ['tag', tag]);
run('git', ['push', 'origin', 'main']);
run('git', ['push', 'origin', tag]);

console.log(`\nPushed ${tag} — CI will build, test, and publish it: https://github.com/yggdrion/win-audio-sessions/actions`);
