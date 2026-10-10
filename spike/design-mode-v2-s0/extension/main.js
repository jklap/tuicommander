// Page hints are untrusted; this script has no extension API access.
const OriginalWebSocket = window.WebSocket;
window.WebSocket = class extends OriginalWebSocket {
  constructor(url, protocols) {
    super(url, protocols);
    if (String(protocols).includes('vite-hmr') || String(url).includes('webpack-hmr')) {
      this.addEventListener('message', event => window.postMessage({s0: true, kind: 'hmr', url: String(url).split('?')[0], sample: String(event.data).slice(0, 200)}, location.origin));
    }
  }
};
window.addEventListener('message', event => {
  if (event.source !== window || event.data?.s0Request !== 'hint') return;
  const el = document.querySelector('#target');
  window.postMessage({s0: true, kind: 'main_hint', source: el?.__vueParentComponent?.type?.__file ?? null}, location.origin);
});
