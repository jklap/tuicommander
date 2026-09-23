// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke, mockRpc } = vi.hoisted(() => ({
	mockInvoke: vi.fn(),
	mockRpc: vi.fn(),
}));

let fileContent = "";

vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

import { MarkdownTab } from "../../components/MarkdownTab/MarkdownTab";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";

describe("MarkdownTab agent review actions", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		mockRpc.mockReset();
		mockInvoke.mockImplementation((command: string) => {
			if (command === "read_file") {
				return Promise.resolve(fileContent);
			}
			return Promise.resolve(undefined);
		});
		mockRpc.mockResolvedValue({ typed: false, queued: 1 });
		fileContent = "<!--tweak:block:c_review @2026-09-23T08:00:00.000Z\nClarify this heading-->\n# Heading";
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		mdTabsStore.clearAll();
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		mdTabsStore.clearAll();
	});

	function addAgent(name: string, sessionId: string, repoPath: string) {
		const id = terminalsStore.add({ name, sessionId, fontSize: 14, cwd: repoPath, awaitingInput: null });
		terminalsStore.update(id, { agentType: "claude", repoPath });
		return id;
	}

	it("queues the marked-up file for the selected agent in the same repository", async () => {
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		addAgent("Other repo", "session-other", "/other");
		addAgent("Reviewer one", "session-one", "/repo");
		addAgent("Reviewer two", "session-two", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const tab = mdTabsStore.get(tabId) as FileTab;

		const { container } = render(() => <MarkdownTab tab={tab} />);
		const selector = await screen.findByRole("combobox", { name: "Review agent" });
		expect(screen.getByText("Agent")).not.toBeNull();
		expect(Array.from((selector as HTMLSelectElement).options).map((option) => option.textContent)).toEqual([
			"Reviewer one",
			"Reviewer two",
		]);

		fireEvent.change(selector, { target: { value: "session-two" } });
		const send = await screen.findByRole("button", { name: "Send changes to agent" });
		await waitFor(() => expect((send as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(send);
		expect((send as HTMLButtonElement).disabled).toBe(true);

		await waitFor(() =>
			expect(mockRpc).toHaveBeenCalledWith("enqueue_agent_command", {
				sessionId: "session-two",
				text: expect.stringContaining("/repo/docs/review.md"),
			}),
		);
		await waitFor(() => expect((send as HTMLButtonElement).disabled).toBe(false));
		expect(addToast).toHaveBeenCalledWith("Sent to agent", expect.stringContaining("queued"), "info");
		expect(container.querySelector(".header")?.lastElementChild?.contains(send)).toBe(true);
	});

	it("hides the agent review controls when the file has no tweak comments", async () => {
		fileContent = "# No pending review\n";
		addAgent("Reviewer", "session-one", "/repo");
		const tabId = mdTabsStore.add("/repo", "docs/clean.md");
		const tab = mdTabsStore.get(tabId) as FileTab;

		render(() => <MarkdownTab tab={tab} />);
		await screen.findByRole("heading", { name: "No pending review" });

		expect(screen.queryByRole("combobox", { name: "Review agent" })).toBeNull();
		expect(screen.queryByRole("button", { name: "Send changes to agent" })).toBeNull();
	});
});
