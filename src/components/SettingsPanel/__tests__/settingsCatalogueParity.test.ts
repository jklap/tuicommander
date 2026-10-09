import { readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { settingsCatalogue } from "../../../generated/settingsCatalogue";
import { hasCatalogueEditor } from "../../../services/settingsCatalogue";
import catalogueExclusions from "../settingsCatalogueExclusions.json";
import controlExclusions from "../settingsControlExclusions.json";
import { extractSettingControls, missingCatalogueControls, missingControlIdentities } from "./settingsControls";

const root = resolve(__dirname, "../../../..");
function files(directory: string): string[] {
	return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
		const path = join(directory, entry.name);
		return entry.isDirectory() ? files(path) : [path];
	});
}
// Shared primitives are audited at their call sites. Production pages, raw controls,
// expert/platform branches and dynamic row bodies are all parsed, even when unmounted.
const sources = [
	...files(join(root, "src/components/SettingsPanel")),
	// Shared by the Remote Servers page's merged editor (the standalone tunnel modal is gone).
	join(root, "src/components/TunnelsPanel/PortForwardsEditor.tsx"),
	join(root, "src/components/TunnelsPanel/TunnelProfileList.tsx"),
	join(root, "src/components/shared/SshConnectionFields.tsx"),
].filter(
	(path) =>
		path.endsWith(".tsx") &&
		!path.includes("/__tests__/") &&
		!path.includes("/fields/") &&
		!path.endsWith("/SettingFields.tsx"),
);
const controls = () =>
	sources.flatMap((path) => extractSettingControls(relative(root, path), readFileSync(path, "utf8")));

describe("settings catalogue/control identity boundary", () => {
	// Catches: a raw input hidden in a dynamic row bypassing the settings inventory.
	it("raw_dynamic_control_missing_id_fails", () => {
		expect(missingControlIdentities(controls(), controlExclusions)).toEqual([]);
		const catalogueIds = new Set(Object.keys(settingsCatalogue.fields));
		expect(controls().filter((control) => control.settingId && !catalogueIds.has(control.settingId))).toEqual([]);
		expect(
			Object.keys(controlExclusions).filter((id) => !controls().some((control) => control.exclusion === id)),
		).toEqual([]);
	});

	// Catches: any computed expression being accepted as a catalogue identity.
	it("rejects_unresolved_dynamic_control_ids", () => {
		const result = extractSettingControls(
			"fixture.tsx",
			`<input data-setting-id={unknown.id} /><button onClick={() => save()}>Save</button>`,
		);
		expect(missingControlIdentities(result).map((control) => control.tag)).toEqual(["input", "button"]);
	});

	// Catches: owner-dependent controls losing either of their finite catalogue identities.
	it("resolves_both_literal_owner_identities", () => {
		const result = extractSettingControls(
			"fixture.tsx",
			`<input data-setting-id={props.agentType ? "agents.agents.{key}.launch_modules.{key}" : "agents.launch_modules.{key}"} />`,
		);
		expect(result.map((control) => control.settingId)).toEqual([
			"agents.agents.{key}.launch_modules.{key}",
			"agents.launch_modules.{key}",
		]);
		expect(missingControlIdentities(result)).toEqual([]);
	});

	// Catches: an extractor skipping JSX in callbacks or stopping at an arrow's >.
	it("finds_raw_controls_inside_dynamic_rows_and_platform_fallbacks", () => {
		const source = `<Show when={isTauri()} fallback={<input data-non-setting="navigation" />}><For each={rows()}>{row => <Input onInput={e => edit(row.id, e.currentTarget.value)} />}</For></Show>`;
		const result = extractSettingControls("fixture.tsx", source);
		expect(result).toHaveLength(2);
		expect(missingControlIdentities(result).map((control) => control.tag)).toEqual(["Input"]);
	});

	// Catches: adding a descriptor without any real control, or silently treating an empty reason as approval.
	it("catalogue_without_ui_fails", () => {
		const production = controls();
		// These IDs are supported by the actual typed renderer, not a hand-maintained
		// claim list. allSettingsTab.test.tsx mounts every supported descriptor.
		if (production.some((control) => control.file.endsWith("/AllSettingsTab.tsx") && control.dynamic)) {
			for (const field of Object.values(settingsCatalogue.fields).filter(hasCatalogueEditor))
				production.push({
					file: "AllSettingsTab",
					line: 0,
					tag: "CatalogueFieldEditor",
					settingId: field.id,
					dynamic: true,
				});
		}
		expect(missingCatalogueControls(Object.keys(settingsCatalogue.fields), production, catalogueExclusions)).toEqual(
			[],
		);
	});

	// Catches: a raw enum select retaining a handwritten choice list after the owner changes it.
	it("ui_options_match_schema", () => {
		const fields = settingsCatalogue.fields as Record<string, { schema: { enum?: readonly unknown[] } }>;
		expect(
			controls().filter(
				(control) =>
					["Select", "select"].includes(control.tag) &&
					control.settingId &&
					fields[control.settingId]?.schema.enum &&
					control.enumSource !== control.settingId,
			),
		).toEqual([]);
	});

	// Catches: removed or empty ledger entries silently authorizing unrelated gaps.
	it("exclusions_name_live_native_fields_with_specific_reasons", () => {
		for (const [id, reason] of Object.entries(catalogueExclusions)) {
			expect(Object.hasOwn(settingsCatalogue.fields, id)).toBe(true);
			expect(reason.trim().length).toBeGreaterThan(0);
		}
	});

	it("empty_exclusion_is_not_coverage", () => {
		const result = extractSettingControls(
			"fixture.tsx",
			`<SettingInput settingId="app.shell" /><Input data-non-setting="search query" />`,
		);
		expect(missingCatalogueControls(["app.shell", "app.theme"], result, { "app.theme": "" })).toEqual(["app.theme"]);
	});
});
