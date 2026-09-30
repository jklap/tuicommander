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

describe("extractTab — client gating", () => {
	it("tags what an isTauri() Show renders, and what its fallback renders instead", () => {
		const src = `
			<h3>Section</h3>
			<Show
				when={isTauri()}
				fallback={<SettingInput label={t("x.path", "Path")} />}
			>
				<label>Picker</label>
				<Show when={ready()}>
					<label>Nested</label>
				</Show>
			</Show>
			<label>Everywhere</label>
		`;
		expect(extractTab(src).settings).toEqual([
			{ key: "x.path", text: "Path", section: "Section", platform: "browser" },
			{ text: "Picker", section: "Section", platform: "desktop" },
			{ text: "Nested", section: "Section", platform: "desktop" },
			{ text: "Everywhere", section: "Section" },
		]);
	});

	it("tags a section heading gated on the desktop, even under a compound condition", () => {
		const src = `<Show when={isTauri() && cliStatus()}><h3>Desktop only</h3></Show><h3>Always</h3>`;
		expect(extractTab(src).sections).toEqual([{ text: "Desktop only", platform: "desktop" }, { text: "Always" }]);
	});

	it("leaves a Show gated on anything but the client untagged", () => {
		const src = `<h3>S</h3><Show when={showAdd()}><label>Form field</label></Show>`;
		expect(extractTab(src).settings).toEqual([{ text: "Form field", section: "S" }]);
	});
});

describe("extractTab — text read from source", () => {
	it("ignores accessible names on form controls while indexing setting labels", () => {
		const src = `<input aria-label="Model" /><h3>Agents</h3><SettingToggle label="Show agent intent as tab title" />`;
		expect(extractTab(src).settings).toEqual([{ text: "Show agent intent as tab title", section: "Agents" }]);
	});
	it("counts a label whose text continues into a runtime expression as dynamic", () => {
		// Rendered as "Discovered tools (3)"; the index could only hold the
		// prefix, which `scrollToSetting` would never find.
		const src = `<h3>S</h3><label>Discovered tools ({tools().length})</label><label>Static</label>`;
		const extracted = extractTab(src);
		expect(extracted.settings).toEqual([{ text: "Static", section: "S" }]);
		expect(extracted.dynamic).toBe(1);
	});
});
