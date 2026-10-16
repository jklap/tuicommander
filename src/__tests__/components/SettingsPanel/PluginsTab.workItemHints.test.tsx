import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../invoke", () => ({
	invoke: vi.fn(async (command: string) => (command === "fetch_plugin_registry" ? [] : null)),
}));

import { PluginsTab } from "../../../components/SettingsPanel/tabs/PluginsTab";
import { pluginStore } from "../../../stores/pluginStore";

afterEach(() => {
	cleanup();
	pluginStore.clear();
});

describe("Plugins settings work-item hints", () => {
	// Catches: Settings listing no hint that Tasks needs a work-item plugin, so the user never finds where to get one.
	it("hints at each work-item plugin that is not installed", () => {
		render(() => <PluginsTab />);
		expect(screen.getByText(/GitHub Work shows GitHub issues/)).toBeInTheDocument();
		expect(screen.getByText(/GitLab shows GitLab issues/)).toBeInTheDocument();
		expect(screen.getByText(/Jira shows Jira issues/)).toBeInTheDocument();
	});

	it("opens only a loaded catalogue page through its named Settings action", () => {
		pluginStore.registerPlugin("fixture", { loaded: true });
		const open = vi.fn();
		render(() => <PluginsTab settingsPluginIds={["fixture"]} onOpenSettings={open} />);
		fireEvent.click(screen.getByRole("button", { name: "Settings" }));
		expect(open).toHaveBeenCalledWith("fixture");
	});

	// Catches: a hint staying on screen for a plugin that is already installed.
	it("drops the hint of an installed work-item plugin", () => {
		pluginStore.registerPlugin("gitlab", { loaded: true });
		render(() => <PluginsTab />);
		expect(screen.queryByText(/GitLab shows GitLab issues/)).not.toBeInTheDocument();
		expect(screen.getByText(/Jira shows Jira issues/)).toBeInTheDocument();
	});
});
