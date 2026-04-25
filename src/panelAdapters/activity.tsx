import { Component, For, Show, createSignal, createEffect, onCleanup, onMount } from "solid-js";
import { invoke } from "../invoke";
import { buildActivitySnapshot, type ActivitySnapshot, type ActivityTerminalRow } from "../utils/activitySnapshot";
import { createPanelSyncReceiver } from "../utils/panelSync";
import { initPanelWindow } from "../hooks/initPanelWindow";
import { terminalsStore } from "../stores/terminals";
import { globalWorkspaceStore } from "../stores/globalWorkspace";
import { formatRelativeTime } from "../utils/time";
import type { PanelAdapter } from "../panelRouter";

function truncate(text: string, maxLen = 80): string {
  const oneLine = text.replace(/\n/g, " ").trim();
  if (oneLine.length <= maxLen) return oneLine;
  return oneLine.slice(0, maxLen - 1) + "…";
}

const DetachedActivityDashboard: Component<{ params: URLSearchParams }> = () => {
  const { state, emitAction } = createPanelSyncReceiver<ActivitySnapshot>("activity");
  const [, setTick] = createSignal(0);

  onMount(() => {
    void initPanelWindow();
  });

  createEffect(() => {
    const interval = setInterval(() => setTick((n) => n + 1), 1000);
    onCleanup(() => clearInterval(interval));
  });

  const handleRowClick = (termId: string) => {
    void emitAction("navigate", { termId });
  };

  const handlePromote = (e: MouseEvent, termId: string) => {
    e.stopPropagation();
    void emitAction("promote", { termId });
  };

  return (
    <div style={{ padding: "12px", height: "100vh", overflow: "auto", background: "var(--bg-primary)", color: "var(--fg-primary)" }}>
      <h3 style={{ margin: "0 0 12px", "font-size": "14px" }}>Activity Dashboard</h3>
      <Show when={state()} fallback={<div style={{ color: "var(--fg-secondary)" }}>Waiting for data...</div>}>
        {(snap) => (
          <For each={snap().terminals}>
            {(term: ActivityTerminalRow) => (
              <div
                onClick={() => handleRowClick(term.id)}
                style={{
                  padding: "8px",
                  "border-bottom": "1px solid var(--border)",
                  cursor: "pointer",
                  background: term.isActive ? "var(--bg-tertiary)" : "transparent",
                }}
              >
                <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
                  <span style={{ "font-weight": "bold", "font-size": "13px" }}>{term.name}</span>
                  <span style={{ color: "var(--fg-secondary)", "font-size": "12px" }}>{term.agentType ?? "shell"}</span>
                  <span style={{
                    "font-size": "11px",
                    color: term.shellState === "busy" ? "var(--fg-accent)" : "var(--fg-secondary)",
                  }}>
                    {term.isRateLimited ? "Rate limited" : term.awaitingInput ? "Waiting" : term.shellState ?? "—"}
                  </span>
                  <span style={{ "margin-left": "auto", "font-size": "11px", color: "var(--fg-secondary)" }}>
                    {formatRelativeTime(term.lastDataAt)}
                  </span>
                  <button
                    onClick={(e) => handlePromote(e, term.id)}
                    style={{
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      color: term.isPromoted ? "var(--fg-accent)" : "var(--fg-secondary)",
                      "font-size": "14px",
                    }}
                    title={term.isPromoted ? "Remove from Global Workspace" : "Promote to Global Workspace"}
                  >
                    {term.isPromoted ? "\u{1F310}" : "\u{1F517}"}
                  </button>
                </div>
                <Show when={term.agentIntent}>
                  <div style={{ "font-size": "12px", color: "var(--fg-secondary)", "margin-top": "4px" }}>
                    {truncate(term.agentIntent!)}
                  </div>
                </Show>
                <Show when={term.currentTask}>
                  <div style={{ "font-size": "12px", color: "var(--fg-secondary)", "margin-top": "2px" }}>
                    {truncate(term.currentTask!)}
                  </div>
                </Show>
              </div>
            )}
          </For>
        )}
      </Show>
    </div>
  );
};

export const activityPanelAdapter: PanelAdapter & {
  syncIntervalMs: number;
  serialize: () => ActivitySnapshot;
  handleAction: (action: string, data: unknown) => void;
} = {
  id: "activity",
  title: "Activity Dashboard",
  defaultSize: { width: 550, height: 650 },
  syncIntervalMs: 1000,
  serialize: buildActivitySnapshot,
  handleAction(action: string, data: unknown) {
    if (action === "navigate") {
      const { termId } = data as { termId: string };
      void invoke("focus_main_window");
      terminalsStore.setActive(termId);
      requestAnimationFrame(() => terminalsStore.get(termId)?.ref?.focus());
    } else if (action === "promote") {
      const { termId } = data as { termId: string };
      globalWorkspaceStore.togglePromote(termId);
    }
  },
  Component: DetachedActivityDashboard,
};
