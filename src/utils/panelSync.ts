import { createSignal, onCleanup } from "solid-js";
import { listen } from "../invoke";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export interface PanelSnapshot<T = unknown> {
  panelId: string;
  ts: number;
  snapshot: T;
}

export interface PanelAction {
  panelId: string;
  action: string;
  data: unknown;
}

export function createPanelSyncReceiver<T>(panelId: string) {
  const [state, setState] = createSignal<T | null>(null);
  let lastTs = 0;

  const cleanups: (() => void)[] = [];

  // Use window-scoped listen — emitTo targets a specific window,
  // so the global listen (broadcast only) won't receive these events.
  const win = getCurrentWebviewWindow();
  win.listen<PanelSnapshot<T>>("panel-sync", (event) => {
    if (event.payload.panelId !== panelId) return;
    if (event.payload.ts <= lastTs) return;
    lastTs = event.payload.ts;
    setState(() => event.payload.snapshot);
  }).then((fn) => cleanups.push(fn));

  const onVisChange = () => {
    if (!document.hidden) {
      emitTo("main", "panel-resync-request", { panelId });
    }
  };
  document.addEventListener("visibilitychange", onVisChange);
  cleanups.push(() => document.removeEventListener("visibilitychange", onVisChange));

  // Notify main window when this panel window is closed via OS controls.
  win.onCloseRequested(async () => {
    await emitTo("main", "panel-window-closed", panelId);
  }).then((fn) => cleanups.push(fn));

  function destroy() {
    for (const fn of cleanups) fn();
    cleanups.length = 0;
  }

  onCleanup(destroy);

  async function emitAction(action: string, data: unknown) {
    await emitTo("main", "panel-action", { panelId, action, data });
  }

  return { state, emitAction, destroy };
}

export function createPanelSyncProvider(
  panelId: string,
  serialize: () => unknown,
  intervalMs: number,
) {
  let timer: ReturnType<typeof setInterval> | undefined;
  let resyncUnlisten: (() => void) | undefined;

  function push() {
    const label = `panel-${panelId}`;
    emitTo(label, "panel-sync", {
      panelId,
      ts: Date.now(),
      snapshot: serialize(),
    }).catch(() => {});
  }

  function start() {
    if (timer) return;
    push();
    timer = setInterval(push, intervalMs);
  }

  listen<{ panelId: string }>("panel-resync-request", (e) => {
    if (e.payload.panelId === panelId) push();
  }).then((fn) => { resyncUnlisten = fn; });

  function stop() {
    clearInterval(timer);
    timer = undefined;
    resyncUnlisten?.();
    resyncUnlisten = undefined;
  }

  return { start, stop, push };
}
