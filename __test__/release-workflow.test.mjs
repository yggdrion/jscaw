import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { test } from 'node:test';

const workflowPath = new URL('../.github/workflows/release.yml', import.meta.url);
const ciPath = new URL('../.github/workflows/CI.yml', import.meta.url);

test('manual release workflow bumps, tags, and dispatches CI', () => {
  assert.ok(existsSync(workflowPath));
  const workflow = readFileSync(workflowPath, 'utf8');

  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /type: choice/);
  assert.match(workflow, /- patch/);
  assert.match(workflow, /- minor/);
  assert.match(workflow, /- major/);
  assert.match(workflow, /actions: write/);
  assert.match(workflow, /pnpm release \$\{\{ inputs\.bump \}\}/);
  assert.match(workflow, /git config user\.name 'github-actions\[bot\]'/);
  assert.match(workflow, /git describe --tags --exact-match/);
  assert.match(workflow, /gh workflow run CI\.yml --ref/);
  const ci = readFileSync(ciPath, 'utf8');
  assert.match(ci, /workflow_dispatch:/);
  assert.match(ci, /if: startsWith\(github\.ref, 'refs\/tags\/v'\)/);
});
