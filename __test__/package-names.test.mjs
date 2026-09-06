import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const readPackage = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));

test('npm package manifests use the yggdrion scope', () => {
  const root = readPackage('../package.json');
  const x64 = readPackage('../npm/win32-x64-msvc/package.json');
  const arm64 = readPackage('../npm/win32-arm64-msvc/package.json');

  assert.equal(root.name, '@yggdrion/jscaw');
  assert.equal(root.napi.packageName, '@yggdrion/jscaw');
  assert.equal(x64.name, '@yggdrion/jscaw-win32-x64-msvc');
  assert.equal(arm64.name, '@yggdrion/jscaw-win32-arm64-msvc');
});
