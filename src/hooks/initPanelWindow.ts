import { settingsStore } from "../stores/settings";
import { applyAppTheme, applyFontFamily } from "../themes";

export async function initPanelWindow(): Promise<void> {
  document.getElementById("splash")?.remove();
  await settingsStore.hydrate().catch(() => {});
  applyAppTheme(settingsStore.state.theme);
  applyFontFamily(settingsStore.state.font);
}
