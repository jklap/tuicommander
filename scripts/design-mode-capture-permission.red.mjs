// Explicit RED probe, not part of the unattended suite: capture permission boundary regression.
// Run: scripts/with-test-tmp.sh node --test scripts/design-mode-capture-permission.red.mjs
import assert from 'node:assert/strict';
import {readFile, writeFile} from 'node:fs/promises';
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
    assert.match(error.message, /Let agents capture any site/);
    return true;
  });
});


// Opt-in real Chrome boundary: run before consent (RED), enable the actual option,
// then run again (GREEN). CDP is test transport only; it never grants permissions.
const endpoint = process.env.TUIC_DESIGN_CDP;
async function cdp(target) {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  let id = 0;
  const pending = new Map();
  ws.onmessage = ({data}) => {
    const message = JSON.parse(data), call = pending.get(message.id);
    if (!call) return;
    pending.delete(message.id); clearTimeout(call.timer);
    message.error ? call.reject(new Error(JSON.stringify(message.error))) : call.resolve(message.result);
  };
  return {
    send(method, params = {}) {
      return new Promise((resolve, reject) => {
        const key = ++id;
        const timer = setTimeout(() => { pending.delete(key); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
        pending.set(key, {resolve, reject, timer}); ws.send(JSON.stringify({id:key, method, params}));
      });
    },
    close() { ws.close(); },
  };
}
test('live Chrome captures the reviewed page without a browser-action gesture after optional consent', {skip:!endpoint}, async () => {
  const targets = await fetch(`${endpoint}/json/list`).then((response) => response.json());
  const panel = targets.find((target) => target.url.startsWith('chrome-extension://') && target.url.endsWith('/panel.html'));
  assert.ok(panel, 'Open the real extension panel document in the owned test profile first.');
  const client = await cdp(panel);
  try {
    const reply = await client.send('Runtime.evaluate', {awaitPromise:true, returnByValue:true, expression:`(async () => {
      const [tab] = await chrome.tabs.query({url:'http://127.0.0.1:19886/*'});
      if (!tab) return {error:'Owned fixture tab missing'};
      await chrome.tabs.update(tab.id, {active:true});
      const {browserCommand} = await import(chrome.runtime.getURL('browserCommands.js'));
      try { return {...await browserCommand({action:'screenshot'}, chrome, tab.id), permissions:await chrome.permissions.getAll()}; }
      catch(error) { return {error:error.message, permissions:await chrome.permissions.getAll()}; }
    })()`});
    assert.equal(reply.exceptionDetails, undefined, JSON.stringify(reply.exceptionDetails));
    const result = reply.result.value;
    assert.equal(result.error, undefined, JSON.stringify(result));
    assert.match(result.image, /^data:image\/png;base64,/);
    assert.ok(Buffer.from(result.image.split(',')[1], 'base64').subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])));
    if (process.env.TUIC_DESIGN_CAPTURE) await writeFile(process.env.TUIC_DESIGN_CAPTURE, Buffer.from(result.image.split(',')[1], 'base64'));
  } finally { client.close(); }
});
