import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { mockInvoke } from "../../mocks/tauri";

// GeneralTab.tsx's onMount fires get_cli_status / get_finder_service_status /
// mdkb_status; none of this component's own tests existed before this file —
// `stores/settings`, `stores/updater`, `themes`, `stores/repoDefaults` are
// mocked in the sibling `GeneralTab.test.tsx` only because that file actually
// tests GitHubTab, not GeneralTab (a pre-existing filename/content mismatch,
// left alone here — out of scope for this feature). GeneralTab itself only
// touches `settingsStore`/`updaterStore`, both safe to use for real: neither
// calls the mocked tauri layer just from being imported or rendered.

const mockIsMacOS = { value: true };
vi.mock("../../../platform", () => ({ isMacOS: () => mockIsMacOS.value }));

import { GeneralTab } from "../../../components/SettingsPanel/tabs/GeneralTab";

function mockInvokeByCommand(handlers: Record<string, unknown>) {
	mockInvoke.mockImplementation((command: string) => {
		if (command in handlers) return Promise.resolve(handlers[command]);
		return Promise.resolve(undefined);
	});
}

describe("GeneralTab — TUIC CLI section", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockIsMacOS.value = true;
	});

	it("offers to install when the CLI is not installed", async () => {
		mockInvokeByCommand({ get_cli_status: { installed: false, prompt_dismissed: false } });
		const { findByText } = render(() => <GeneralTab />);

		expect(await findByText("Install TUIC CLI")).toBeTruthy();
	});

	it("shows the installed path and an uninstall control once installed", async () => {
		mockInvokeByCommand({
			get_cli_status: { installed: true, path: "/usr/local/bin/tuic", version_match: true, auto_updatable: true },
		});
		const { findByText } = render(() => <GeneralTab />);

		expect(await findByText(/Installed at \/usr\/local\/bin\/tuic/)).toBeTruthy();
		expect(await findByText("Uninstall")).toBeTruthy();
	});

	it("calls install_cli when the install button is clicked", async () => {
		mockInvokeByCommand({ get_cli_status: { installed: false, prompt_dismissed: false } });
		const { findByText } = render(() => <GeneralTab />);

		fireEvent.click(await findByText("Install TUIC CLI"));

		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("install_cli"));
	});

	it("calls uninstall_cli when the uninstall button is clicked", async () => {
		mockInvokeByCommand({
			get_cli_status: { installed: true, path: "/usr/local/bin/tuic", version_match: true, auto_updatable: true },
		});
		const { findByText } = render(() => <GeneralTab />);

		fireEvent.click(await findByText("Uninstall"));

		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("uninstall_cli"));
	});
});

describe("GeneralTab — Finder Integration section (macOS)", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockIsMacOS.value = true;
	});

	it("does not render at all on a non-macOS platform", async () => {
		mockIsMacOS.value = false;
		mockInvokeByCommand({ get_finder_service_status: { installed: false, prompt_dismissed: false } });
		const { queryByText } = render(() => <GeneralTab />);

		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_cli_status"));
		expect(mockInvoke).not.toHaveBeenCalledWith("get_finder_service_status");
		expect(queryByText("Finder Integration")).toBeNull();
	});

	it("offers to add the integration when not installed", async () => {
		mockInvokeByCommand({ get_finder_service_status: { installed: false, prompt_dismissed: false } });
		const { findByText } = render(() => <GeneralTab />);

		expect(await findByText("Add Finder Integration")).toBeTruthy();
	});

	it("shows installed state and a remove control once installed", async () => {
		mockInvokeByCommand({ get_finder_service_status: { installed: true, prompt_dismissed: true } });
		const { findByText } = render(() => <GeneralTab />);

		expect(await findByText(/Installed/)).toBeTruthy();
		expect(await findByText("Remove")).toBeTruthy();
	});

	it("calls install_finder_service when the install button is clicked", async () => {
		mockInvokeByCommand({ get_finder_service_status: { installed: false, prompt_dismissed: false } });
		const { findByText } = render(() => <GeneralTab />);

		fireEvent.click(await findByText("Add Finder Integration"));

		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("install_finder_service"));
	});

	it("calls uninstall_finder_service when the remove button is clicked", async () => {
		mockInvokeByCommand({ get_finder_service_status: { installed: true, prompt_dismissed: true } });
		const { findByText } = render(() => <GeneralTab />);

		fireEvent.click(await findByText("Remove"));

		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("uninstall_finder_service"));
	});
});
