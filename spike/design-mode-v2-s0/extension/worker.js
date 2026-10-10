// Echo spike only. Telemetry goes to our explicit loopback evidence collector.
const nonce = crypto.randomUUID();
const report = (event) => fetch('http://127.0.0.1:19341/events', {
  method: 'POST', headers: {'Content-Type': 'application/json'},
  body: JSON.stringify({nonce, ua: navigator.userAgent, time: Date.now(), ...event}),
}).catch(() => {});
report({kind: 'worker_boot', sidePanel: typeof chrome.sidePanel?.open});
const native = chrome.runtime.connectNative('dev.tuic.dm2_s0');
native.onMessage.addListener((message) => report({kind: 'native_reply', message}));
native.onDisconnect.addListener(() => report({kind: 'native_disconnect', error: chrome.runtime.lastError?.message}));
native.postMessage({nonce, delayed: true});
chrome.sidePanel.setPanelBehavior({openPanelOnActionClick: true}).catch((error) => report({kind: 'panel_error', error: String(error)}));
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (message.kind === 'arm') {
    chrome.tabs.query({active: true, currentWindow: true}).then(([tab]) => chrome.tabs.sendMessage(tab.id, {kind: 'arm'}));
  } else if (message.kind === 'capture') {
    chrome.tabs.captureVisibleTab(sender.tab.windowId, {format: 'png'}).then(async (data) => {
      const bitmap = await createImageBitmap(await (await fetch(data)).blob());
      const {rect, dpr} = message;
      const canvas = new OffscreenCanvas(Math.round(rect.width * dpr), Math.round(rect.height * dpr));
      canvas.getContext('2d').drawImage(bitmap, rect.x * dpr, rect.y * dpr, rect.width * dpr, rect.height * dpr, 0, 0, canvas.width, canvas.height);
      const blob = await canvas.convertToBlob({type: 'image/png'});
      report({kind: 'capture', frameId: sender.frameId, width: canvas.width, height: canvas.height, bytes: blob.size, viewport: [bitmap.width, bitmap.height], dpr});
      // Save crop bytes in the evidence collector, no page or TUIC filesystem access.
      fetch('http://127.0.0.1:19341/crop', {method: 'POST', body: blob});
    }).catch((error) => report({kind: 'capture_error', error: String(error)}));
  } else {
    report({...message, frameId: sender.frameId, documentId: sender.documentId});
    if (message.kind === 'scripts') {
      for (const url of message.urls) fetch(url).then(r => r.text()).then(text => {
        const match = text.match(/[#@] sourceMappingURL=([^\s]+)/);
        report({kind: 'source_map', url, found: !!match, reference: match?.[1]?.slice(0, 100)});
      }).catch(error => report({kind: 'source_map_error', error: String(error)}));
    }
  }
  reply({ok: true});
});
