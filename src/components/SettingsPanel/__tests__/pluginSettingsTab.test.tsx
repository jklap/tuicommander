import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { mockInvoke } from "../../../__tests__/mocks/tauri";
import { pluginStore } from "../../../stores/pluginStore";
import { SettingsPanel } from "../SettingsPanel";

vi.mock("../tabs", () => ({ GeneralTab: () => <h3>General</h3> }));
vi.mock("../../../stores/repositories", () => ({ repositoriesStore: { getAllReposOrdered: () => [] } }));
vi.mock("../../../stores/settings", () => ({ settingsStore: { isAiChatEnabled: () => false } }));
vi.mock("../ExpertSetting", () => ({ ExpertModeSwitch: () => null }));
vi.mock("../../../stores/settingsExpert", () => ({ settingsExpertStore: { open: () => Promise.resolve() } }));

const field = {
	id: "plugins.fixture.endpoint",
	path: "/endpoint",
	label: "endpoint",
	schema: { type: "string", title: "Server endpoint" },
	operations: ["get", "set", "reset"],
	sensitivity: "public",
	order: 0,
};
const catalogue = { fields: { [field.id]: field }, collections: [], gaps: [], exclusions: [] };

beforeEach(() => {
	pluginStore.clear();
	pluginStore.registerPlugin("fixture", {
		loaded: true,
		manifest: {
			id: "fixture",
			name: "Fixture integration",
			version: "1.0.0",
			minAppVersion: "1.0.0",
			description: "Fixture",
			main: "main.js",
			capabilities: [],
			settings: {
				schema: {
					type: "object",
					additionalProperties: false,
					properties: {
						endpoint: {
							type: "string",
							title: "Server endpoint",
							"x-setting": {
								sensitivity: "public",
								apply: "live",
								effect: "Endpoint changes",
								operations: ["get", "set", "reset"],
								confirmationClass: "ordinary",
							},
						},
					},
				},
				defaults: { endpoint: "https://example.test" },
			},
		},
	});
	mockInvoke.mockReset();
	mockInvoke.mockImplementation(async (command, args) => {
		if (command !== "plugin_settings_action") return undefined;
		if (args.action.action === "describe") return catalogue;
		if (args.action.action === "get")
			return {
				revision: "r1",
				values: {
					"/endpoint": { stored: "https://example.test", effective: "https://example.test", source: "stored" },
				},
			};
		return { revision: "r2", persisted: true, effects: [], restart: [] };
	});
});

it("unloaded_plugin_tab_disappears — catches stale tab writing to a dead plugin", async () => {
	const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />);
	await waitFor(() =>
		expect((view.container.querySelector("[data-settings-content] input[type=text]") as HTMLInputElement)?.value).toBe(
			"https://example.test",
		),
	);
	pluginStore.updatePlugin("fixture", { loaded: false });
	await waitFor(() => expect(view.container.textContent).not.toContain("Fixture integration"));
	expect(view.container.querySelector("[data-settings-content] input[type=text]")).toBeNull();
});

it("plugin_tab_is_searchable — catches search index built only from static tabs", async () => {
	const view = render(() => <SettingsPanel visible onClose={() => {}} />);
	await waitFor(() => expect(view.container.textContent).toContain("Fixture integration"));
	fireEvent.input(view.container.querySelector("nav input")!, { target: { value: "Server endpoint" } });
	await waitFor(() =>
		expect(view.container.querySelector("[data-settings-content]")?.textContent).toContain("Server endpoint"),
	);
});

it("plugin_tab_write_goes_through_settings_service — catches a second settings path", async () => {
	const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />);
	await waitFor(() =>
		expect((view.container.querySelector("[data-settings-content] input[type=text]") as HTMLInputElement)?.value).toBe(
			"https://example.test",
		),
	);
	const input = view.container.querySelector("[data-settings-content] input[type=text]")!;
	fireEvent.input(input, { target: { value: "https://changed.test" } });
	fireEvent.blur(input);
	await waitFor(() =>
		expect(mockInvoke).toHaveBeenCalledWith("plugin_settings_action", {
			pluginId: "fixture",
			action: { action: "set", path: "/endpoint", value: "https://changed.test", expectedRevision: "r1" },
		}),
	);
	expect(mockInvoke.mock.calls.some(([command]) => command === "write_plugin_data")).toBe(false);
});

it("unloaded plugin search entries disappear — catches a retained catalogue exposing dead pages", async () => {
	const view = render(() => <SettingsPanel visible onClose={() => {}} />);
	await waitFor(() => expect(view.container.textContent).toContain("Fixture integration"));
	fireEvent.input(view.container.querySelector("nav input")!, { target: { value: "Server endpoint" } });
	await waitFor(() =>
		expect(view.container.querySelector("[data-settings-content]")?.textContent).toContain("Server endpoint"),
	);
	pluginStore.updatePlugin("fixture", { loaded: false });
	await waitFor(() =>
		expect(view.container.querySelector("[data-settings-content]")?.textContent).not.toContain("Server endpoint"),
	);
});

it("an unchanged secret is not cleared on blur — catches submitting the redacted empty draft", async () => {
	mockInvoke.mockImplementation(async (command, args) => {
		if (command !== "plugin_settings_action") return undefined;
		if (args.action.action === "describe") return { fields: { secret: { ...field, sensitivity: "secret" } } };
		return { revision: "r1", values: { "/endpoint": { effective: { status: "set" } } } };
	});
	const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />);
	await waitFor(() => expect(view.container.querySelector("input[type=password]")).not.toBeNull());
	const input = view.container.querySelector("input[type=password]") as HTMLInputElement;
	expect(input.value).toBe("");
	expect(input.placeholder).toBe("set");
	fireEvent.blur(input);
	expect(mockInvoke.mock.calls.some(([, args]) => args?.action?.action === "set")).toBe(false);
});

it("rejected writes reload the revision without echoing backend errors — catches stale revision retries and private error leaks", async () => {
	let reads = 0;
	mockInvoke.mockImplementation(async (command, args) => {
		if (command !== "plugin_settings_action") return undefined;
		if (args.action.action === "describe") return catalogue;
		if (args.action.action === "get")
			return { revision: `r${++reads}`, values: { "/endpoint": { effective: "https://example.test" } } };
		throw new Error("private credential");
	});
	const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />);
	await waitFor(() =>
		expect((view.container.querySelector("[data-settings-content] input[type=text]") as HTMLInputElement)?.value).toBe(
			"https://example.test",
		),
	);
	const input = view.container.querySelector("[data-settings-content] input[type=text]")!;
	fireEvent.input(input, { target: { value: "https://changed.test" } });
	fireEvent.blur(input);
	await waitFor(() => expect(reads).toBe(2));
	expect(view.container.textContent).not.toContain("private credential");
	expect(view.container.textContent).toContain("Setting was not saved");
	await waitFor(() =>
		expect(
			(view.container.querySelector("[data-settings-content] input[type=text]") as HTMLInputElement)?.disabled,
		).toBe(false),
	);
	const retryInput = view.container.querySelector("[data-settings-content] input[type=text]")!;
	fireEvent.input(retryInput, { target: { value: "https://retry.test" } });
	fireEvent.blur(retryInput);
	await waitFor(() =>
		expect(mockInvoke).toHaveBeenCalledWith("plugin_settings_action", {
			pluginId: "fixture",
			action: { action: "set", path: "/endpoint", value: "https://retry.test", expectedRevision: "r2" },
		}),
	);
});
