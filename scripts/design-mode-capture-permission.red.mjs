// Explicit RED probe, not part of the unattended suite: permission UX awaits approval.
// Run: scripts/with-test-tmp.sh node --test scripts/design-mode-capture-permission.red.mjs
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import test from 'node:test';
import {browserCommand} from '../browser-extension/design-mode/browserCommands.js';

const capture = JSON.parse(await readFile(new URL('./fixtures/design-mode-chrome149-capture-denial.json', import.meta.url), 'utf8'));
const recordedError = JSON.parse(capture.result.content[0].text).error;

test('screenshot permission denial tells the agent how to obtain a user grant', async () => {
  // Catches: raw Chrome permission denial escapes without a typed, actionable recovery.
  // Chrome149 response is recorded, not an invented browser permission model.
  const chrome = {
    tabs: {
      get: async () => ({id:7, active:true, windowId:2, url:'http://localhost:5173/'}),
      captureVisibleTab: async () => { throw new Error(recordedError); },
    },
  };
  await assert.rejects(browserCommand({action:'screenshot'}, chrome, 7), (error) => {
    assert.match(error.message, /^capture_permission_required:/);
    assert.match(error.message, /(?:click|invoke).*Design Mode/i);
    return true;
  });
});

// DEFERRED (2026-10-22) — no production fix in this evidence-only round. Boss must
// choose gesture-based capture or approve broader capture permission first.
