// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import MobileApp from "../MobileApp";
import { FilesScreen } from "../screens/FilesScreen";

const network = vi.hoisted(() => ({ offline: true, repositoriesFailed: false }));
vi.mock("../../transport", () => ({
	isTauri: () => false,
	rpc: vi.fn(async (command: string) => {
		if (network.offline) throw new TypeError("Load failed");
		if (command === "list_active_sessions") return [];
		if (command === "load_repositories" && network.repositoriesFailed) throw new TypeError("Load failed");
		if (command === "load_repositories") return { repos: { "/recovered-project": {} } };
		return {};
	}),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../mobileTheme", () => ({ loadMobileTheme: vi.fn(async () => "commander") }));
vi.mock("../useMobileNotifications", () => ({ useMobileNotifications: vi.fn() }));
vi.mock("../useVersionCheck", () => ({
	useVersionCheck: () => ({ updateAvailable: () => false, serverDown: () => true, applyUpdate: vi.fn() }),
}));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

beforeEach(async () => {
	network.offline = true;
	network.repositoriesFailed = false;
	history.replaceState(null, "", "/mobile");
	await import("../screens/FilesScreen");
});
afterEach(cleanup);

describe("mobile outage recovery", () => {
	// Catches: Files confuses an unsuccessful repository read with an empty successful list and never retries.
	it("retries a failed repository read on recovery without claiming an empty configuration", async () => {
		const [revision, setRevision] = createSignal(0);
		const view = render(() => <FilesScreen recoveryRevision={revision()} />);
		await waitFor(() => expect(view.getByRole("alert")).toBeTruthy());
		expect(view.queryByText("No repositories configured")).toBeNull();
		expect(view.getByRole("alert").textContent).not.toContain("TypeError");
		network.offline = false;
		setRevision(1);
		await waitFor(() => expect(view.getByRole("button", { name: /recovered-project/ })).toBeTruthy());
	});
	// Catches: failed mount-only repository reads never run again when Retry now restores sessions.
	it("reloads Files after manual retry and clears stale version-health offline state", async () => {
		const view = render(() => <MobileApp />);
		await waitFor(() => expect(view.getByText(/Server unreachable/)).toBeTruthy());
		fireEvent.click(view.getByRole("button", { name: "Files" }));
		await waitFor(() => expect(view.queryByText("No repositories configured")).toBeNull());
		expect(view.queryByText(/TypeError: Load failed/)).toBeNull();
		network.offline = false;
		fireEvent.click(view.getByText("Retry now"));
		await waitFor(() => expect(view.getByRole("button", { name: /recovered-project/ })).toBeTruthy());
		expect(view.queryByText(/Server unreachable/)).toBeNull();
	});
	// Catches: online/automatic recovery refreshes sessions but leaves the selected view stuck.
	it("reloads the selected Files view after an online recovery", async () => {
		const view = render(() => <MobileApp />);
		await waitFor(() => expect(view.getByText(/Server unreachable/)).toBeTruthy());
		fireEvent.click(view.getByRole("button", { name: "Files" }));
		network.offline = false;
		window.dispatchEvent(new Event("online"));
		await waitFor(() => expect(view.getByRole("button", { name: /recovered-project/ })).toBeTruthy());
		expect(view.queryByText(/Server unreachable/)).toBeNull();
	});
});
