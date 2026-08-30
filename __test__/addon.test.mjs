import { test } from 'node:test';
import assert from 'node:assert/strict';
import { addonLoaded } from '../index.js';

test('native addon loads', () => {
  assert.equal(addonLoaded(), true);
});
