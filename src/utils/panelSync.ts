import { createSignal } from "solid-js";
import { listen } from "../invoke";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export interface PanelSnapshot<T = unknown> {
  panelId: string;
  seq: number;
  snapshot: T;
}

export interface PanelAction {
  panelId: string;
  action: string;
  data: unknown;
}

export function createPanelSyncReceiver<T>(panelId: string) {
  const [state, setState] = createSignal<T | null>(null);
  let lastSeq = -1;

  // Use window-scoped listen — emitTo targets a specific window,
  // so the global listen (broadcast only) won't receive these events.
  const win = getCurrentWebviewWindow();
  win.listen<PanelSnapshot<T>>("panel-sync", (event) => {
    if (event.payload.panelId !== panelId) return;
    if (event.payload.seq <= lastSeq) return;
    lastSeq = event.payload.seq;
    setState(() => event.payload.snapshot);
  });

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      emitTo("main", "panel-resync-request", { panelId });
    }
  });

  // Notify main window when this panel window is closed (OS close button).
  // Rust on_window_event may not fire reliably for OS-initiated close on macOS.
  window.addEventListener("beforeunload", () => {
    emitTo("main", "panel-window-closed", panelId);
  });

  async function emitAction(action: string, data: unknown) {
    await emitTo("main", "panel-action", { panelId, action, data });
  }

  return { state, emitAction };
}

export function createPanelSyncProvider(
  panelId: string,
  serialize: () => unknown,
  intervalMs: number,
) {
  let seq = 0;
  let timer: ReturnType<typeof setInterval> | undefined;

  function push() {
    const label = `panel-${panelId}`;
    emitTo(label, "panel-sync", { panelId, seq: ++seq, snapshot: serialize() });
  }

  function start() {
    push();
    timer = setInterval(push, intervalMs);
  }

  function stop() {
    clearInterval(timer);
  }

  listen<{ panelId: string }>("panel-resync-request", (e) => {
    if (e.payload.panelId === panelId) push();
  });

  return { start, stop, push };
}
