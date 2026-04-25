import { Component, JSX } from "solid-js";

export interface PanelAdapter {
  id: string;
  title: string;
  defaultSize: { width: number; height: number };
  Component: Component<{ params: URLSearchParams }>;
}

const panelRegistry: Record<string, PanelAdapter> = {};

export function registerPanel(adapter: PanelAdapter): void {
  panelRegistry[adapter.id] = adapter;
}

export function getPanelParams(): {
  isPanelMode: boolean;
  panelId: string | null;
  params: URLSearchParams;
} {
  const params = new URLSearchParams(window.location.search);
  return {
    isPanelMode: params.get("mode") === "panel",
    panelId: params.get("panel"),
    params,
  };
}

export function renderPanelMode(): JSX.Element | null {
  const { isPanelMode, panelId, params } = getPanelParams();
  if (!isPanelMode || !panelId) return null;

  const adapter = panelRegistry[panelId];
  if (!adapter) return null;

  return (
    <div id="app" class={`panel-mode panel-${panelId}`}>
      <adapter.Component params={params} />
    </div>
  );
}

export { panelRegistry };
