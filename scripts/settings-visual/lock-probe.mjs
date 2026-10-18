// Keep the screenshot prerequisite separate from Chrome startup so failed probes
// can be verified without opening a browser or changing the console session.
export function lockProbeFailure(lock) {
  const state = /<key>CGSSessionScreenIsLocked<\/key>\s*<(true|false)\/>/.exec(lock.stdout ?? '');
  return lock.error ? 'macOS lock probe failed: ' + lock.error.message
    : lock.status !== 0 ? 'macOS lock probe failed with status ' + lock.status
    : !state ? 'macOS lock probe returned no usable lock state'
    : state[1] === 'true' ? 'macOS screen is locked; unlock the display before screenshot or rAF checks'
    : null;
}
