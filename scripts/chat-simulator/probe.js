// Measurement-only injection. Never included in the application bundle.
(() => {
  let pending;
  let serial = 0;
  const report = (value) => {
    fetch('/__1770/event', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) }).catch(() => {});
  };
  const info = () => ({ standalone: navigator.standalone === true, displayStandalone: matchMedia('(display-mode: standalone)').matches, userAgent: navigator.userAgent, width: innerWidth, height: innerHeight });
  report({ kind: 'load', ...info() });
  function start(kind) {
    const run = ++serial;
    pending = { kind, start: performance.now(), run };
    requestAnimationFrame(function check() {
      if (!pending || pending.run !== run) return;
      const transcript = document.querySelector('[aria-label="Chat transcript"]');
      const cli = kind === 'cli';
      let last;
      if (transcript) {
        const walker = document.createTreeWalker(transcript, NodeFilter.SHOW_TEXT);
        for (let node; (node = walker.nextNode());) if (node.textContent.includes('SIM1770_LAST_ENTRY')) last = node.parentElement;
      }
      const bounds = last?.getBoundingClientRect();
      const visible = bounds && bounds.height > 0 && bounds.bottom > 0 && bounds.top < innerHeight;
      if ((cli && !transcript) || (!cli && visible)) {
        requestAnimationFrame(() => {
          if (!pending || pending.run !== run) return;
          const elapsed = performance.now() - pending.start;
          const text = transcript?.textContent ?? '';
          const entryCount = new Set(text.match(/SIM1770_ENTRY_\d+/g) ?? []).size;
          report({ kind, elapsedMs: elapsed, entryCount, lastBounds: bounds?.toJSON(), ...info() });
          let ready = document.getElementById('sim1770-ready');
          if (!ready) { ready = document.createElement('output'); ready.id = 'sim1770-ready'; ready.style.cssText = 'position:fixed;top:0;left:0;font-size:1px;pointer-events:none;z-index:2147483647'; document.body.append(ready); }
          ready.textContent = `SIM1770_READY_${kind}_${run}`;
          pending = undefined;
        });
      } else if (performance.now() - pending.start < 90000) requestAnimationFrame(check);
      else { report({ kind: 'timeout', transition: kind, ...info() }); pending = undefined; }
    });
  }
  document.addEventListener('click', (event) => {
    const button = event.target.closest('button');
    if (!button || !event.isTrusted || !button.closest('[aria-label="Terminal view"]')) return;
    const label = button.textContent.trim();
    report({ kind: 'tap', label, trusted: event.isTrusted });
    if (label === 'Chat') start('chat');
    if (label === 'CLI') start('cli');
  }, true);
  document.addEventListener('visibilitychange', () => {
    report({ kind: 'visibility', state: document.visibilityState });
    if (document.visibilityState === 'visible' && document.querySelector('[aria-label="Chat transcript"]')) start('resume');
  });
})();
