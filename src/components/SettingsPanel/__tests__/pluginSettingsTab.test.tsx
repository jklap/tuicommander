import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { ErrorBoundary } from "solid-js";
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

// Catches: saving each list keystroke disables the focused row before typing can finish.
it("string-list typing stays enabled until the edit is committed — catches a save disabling the row after its first character", async () => {
	let finishSave = () => {};
	const pendingSave = new Promise<{ effects: string[]; restart: string[] }>((resolve) => {
		finishSave = () => resolve({ effects: [], restart: [] });
	});
	const listField = {
		...field,
		schema: { type: "array", title: "Allowed projects", maxItems: 128, items: { type: "string" } },
	};
	mockInvoke.mockImplementation(async (command, args) => {
		if (command !== "plugin_settings_action") return undefined;
		if (args.action.action === "describe") return { fields: { list: listField } };
		if (args.action.action === "get") return { revision: "r1", values: { "/endpoint": { effective: ["project"] } } };
		return pendingSave;
	});
	const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />);
	await waitFor(() => expect(view.getByDisplayValue("project")).toBeTruthy());
	const input = view.getByDisplayValue("project") as HTMLInputElement;
	input.focus();
	try {
		fireEvent.input(input, { target: { value: "project-" } });
		expect(input.disabled).toBe(false);
	} finally {
		finishSave();
		await waitFor(() =>
			expect((view.getByRole("textbox", { name: "Value" }) as HTMLInputElement).disabled).toBe(false),
		);
	}
});

// Catches: the resource getter throws past the panel's own read-error UI.
it("a failed settings read leaves the panel usable — catches a rejected resource escaping the local read-error alert", async () => {
	mockInvoke.mockImplementation(async (command, args) => {
		if (command !== "plugin_settings_action") return undefined;
		if (args.action.action === "describe") return catalogue;
		throw new Error("settings unavailable");
	});
	const view = render(() => (
		<ErrorBoundary fallback={<p>Settings crashed</p>}>
			<SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />
		</ErrorBoundary>
	));
	await waitFor(() => expect(view.container.textContent).toContain("Could not read plugin settings."));
	expect(view.container.textContent).not.toContain("Settings crashed");
});

it("string-list draft includes completed additions and removals — catches validating an empty added row or saving a partial edit", async () => {
	const listField = {
		...field,
		schema: { type: "array", title: "Allowed projects", items: { type: "string", minLength: 1 } },
	};
	mockInvoke.mockImplementation(async (command, args) => {
		if (command !== "plugin_settings_action") return undefined;
		if (args.action.action === "describe") return { fields: { list: listField } };
		if (args.action.action === "get") return { revision: "r1", values: { "/endpoint": { effective: ["old"] } } };
		return { effects: [], restart: [] };
	});
	const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />);
	await waitFor(() => expect(view.getByDisplayValue("old")).toBeTruthy());
	const add = view.getByRole("button", { name: "Add row" });
	add.focus();
	fireEvent.click(add);
	const input = view.getAllByRole("textbox", { name: "Value" })[1] as HTMLInputElement;
	input.focus();
	fireEvent.input(input, { target: { value: "new-" } });
	fireEvent.input(input, { target: { value: "new-project" } });
	const remove = view.getByRole("button", { name: "Remove row 0" });
	remove.focus();
	fireEvent.click(remove);
	expect(mockInvoke.mock.calls.some(([, args]) => args?.action?.action === "set")).toBe(false);
	fireEvent.focusOut(add, { relatedTarget: null });
	await waitFor(() =>
		expect(mockInvoke).toHaveBeenCalledWith("plugin_settings_action", {
			pluginId: "fixture",
			action: { action: "set", path: "/endpoint", value: ["new-project"], expectedRevision: "r1" },
		}),
	);
});

it("a failed post-write reload stays local — catches an unhandled refetch rejection replacing Settings", async () => {
	let reads = 0;
	mockInvoke.mockImplementation(async (command, args) => {
		if (command !== "plugin_settings_action") return undefined;
		if (args.action.action === "describe") return catalogue;
		if (args.action.action === "get") {
			if (++reads > 1) throw new Error("private read failure");
			return { revision: "r1", values: { "/endpoint": { effective: "https://example.test" } } };
		}
		return { effects: [], restart: [] };
	});
	const view = render(() => (
		<ErrorBoundary fallback={<p>Settings crashed</p>}>
			<SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />
		</ErrorBoundary>
	));
	await waitFor(() => expect(view.getByDisplayValue("https://example.test")).toBeTruthy());
	const input = view.getByDisplayValue("https://example.test");
	fireEvent.input(input, { target: { value: "https://changed.test" } });
	fireEvent.blur(input);
	await waitFor(() => expect(view.container.textContent).toContain("Could not read plugin settings."));
	expect(view.container.textContent).not.toContain("Settings crashed");
	expect(view.container.textContent).not.toContain("private read failure");
});

// Catches: Reset rendered outside the shared styled, vertically spaced Settings actions row.
it.each(["string", "integer", "boolean"])(
	"plugin Reset (%s) uses the shared Settings action row — catches browser-default buttons touching the next label",
	async (type) => {
		mockInvoke.mockImplementation(async (command, args) => {
			if (command !== "plugin_settings_action") return undefined;
			if (args.action.action === "describe")
				return { fields: { endpoint: { ...field, schema: { ...field.schema, type } } } };
			if (args.action.action === "get")
				return {
					revision: "r1",
					values: {
						"/endpoint": { effective: type === "boolean" ? true : type === "integer" ? 30 : "https://example.test" },
					},
				};
			return { effects: [], restart: [] };
		});
		const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="plugin:fixture" />);
		const reset = await view.findByRole("button", { name: "Reset Server endpoint" });
		expect(reset.parentElement).toHaveClass("actions");
		fireEvent.click(reset);
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("plugin_settings_action", {
				pluginId: "fixture",
				action: { action: "reset", path: "/endpoint", expectedRevision: "r1" },
			}),
		);
	},
);
