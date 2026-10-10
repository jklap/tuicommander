const send = message => chrome.runtime.sendMessage(message).catch(() => {});
window.addEventListener('message', event => {
  if (event.source === window && event.data?.s0) send(event.data);
});
let armed = false;
let overlay;
function stop() { armed = false; overlay?.remove(); overlay = undefined; }
function arm() {
  stop(); armed = true;
  overlay = document.createElement('div');
  const shadow = overlay.attachShadow({mode: 'closed'});
  const label = document.createElement('span');
  label.textContent = 'S0: click one element; Escape cancels';
  shadow.append(label);
  overlay.style.cssText = 'position:fixed;top:0;right:0;z-index:2147483647;pointer-events:none;background:#252526;color:#cccccc;padding:6px;font:12px monospace';
  document.documentElement.append(overlay);
}
chrome.runtime.onMessage.addListener(message => { if (message.kind === 'arm') arm(); });
window.addEventListener('keydown', event => { if (event.key === 'Escape') stop(); }, true);
window.addEventListener('click', event => {
  if (!armed) return;
  event.preventDefault(); event.stopImmediatePropagation();
  stop();
  send({kind: 'pick', payload: tuicExtract.call(event.target)});
}, true);
window.addEventListener('DOMContentLoaded', () => {
  const target = document.querySelector('#target');
  send({kind: 'content_ready', url: location.href, webdriver: navigator.webdriver, strict: document.querySelector('meta[http-equiv]')?.content});
  if (target) {
    send({kind: 'plain_extract', payload: tuicExtract.call(target)});
    if (window === top) send({kind: 'capture', rect: target.getBoundingClientRect().toJSON(), dpr: devicePixelRatio});
  }
  send({kind: 'scripts', urls: Array.from(document.scripts, s => s.src).filter(Boolean)});
  window.postMessage({s0Request: 'hint'}, location.origin);
  // Explicit fixture-only event probe; does not claim a trusted user gesture.
  if (location.search === '?s0pick=1' && target) {
    arm();
    target.dispatchEvent(new MouseEvent('click', {bubbles: true, cancelable: true}));
    send({kind: 'picker_stopped', armed});
  }
});
