// @vitest-environment happy-dom
import {readFileSync} from 'node:fs';
import {describe, expect, it, vi} from 'vitest';

function load() {
  const sendMessage = vi.fn(() => Promise.resolve());
  (globalThis as any).chrome = {runtime: {sendMessage}};
  let handler: (event: unknown) => void = () => {};
  vi.spyOn(window, 'addEventListener').mockImplementation(((_type: string, listener: (event: unknown) => void) => { handler = listener; }) as never);
  new Function(readFileSync('browser-extension/design-mode/errorsBridge.js', 'utf8'))();
  vi.restoreAllMocks();
  const post = (record: object) => handler({source: window, data: {tuicDesignError: record}});
  return {sendMessage, post};
}

describe('errorsBridge', () => {
  // Catches: the per-document seen-set swallowing an error that recurs after the user sent or cleared it,
  // so the sidebar shows no error although the page still throws (HMR fix attempt, same message).
  it('forwards a recurring error every time, not only the first', () => {
    const {sendMessage, post} = load();
    post({kind: 'error', message: 'x is not defined', source: 'http://localhost:5173/a.js'});
    post({kind: 'error', message: 'x is not defined', source: 'http://localhost:5173/a.js'});
    expect(sendMessage).toHaveBeenCalledTimes(2);
  });

  // Catches: a long-lived dev page (HMR, no reload) that has produced 50 distinct errors never reporting a 51st.
  it('keeps forwarding new distinct errors after fifty', () => {
    const {sendMessage, post} = load();
    for (let n = 0; n < 51; n++) post({kind: 'error', message: `e${n}`, source: 's'});
    expect(sendMessage).toHaveBeenCalledTimes(51);
  });
});
