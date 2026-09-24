import { describe, expect, it } from "vitest";
import { extractTab } from "./extractSettings";

describe("extractTab — expert controls", () => {
	it("tags a label inside an ExpertSetting with its configKey", () => {
		const src = `
			<h3>{t("x.heading", "Section")}</h3>
			<SettingToggle label={t("x.basic", "Basic toggle")} />
			<ExpertSetting configKey="app.osc52_clipboard" value={settingsStore.state.osc52Clipboard}>
				<SettingToggle label={t("x.expert", "Expert toggle")} />
			</ExpertSetting>
			<label>After</label>
		`;
		expect(extractTab(src).settings).toEqual([
			{ key: "x.basic", text: "Basic toggle", section: "Section" },
			{ key: "x.expert", text: "Expert toggle", section: "Section", configKey: "app.osc52_clipboard" },
			{ text: "After", section: "Section" },
		]);
	});

	it("rejects an ExpertSetting whose configKey is not a string literal", () => {
		const src = `<h3>S</h3><ExpertSetting configKey={key} value={1}><label>L</label></ExpertSetting>`;
		expect(() => extractTab(src)).toThrow(/configKey/);
	});
});
