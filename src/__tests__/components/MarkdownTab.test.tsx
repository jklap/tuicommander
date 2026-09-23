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
import { repositoriesStore } from "../../stores/repositories";
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

	/** Render `docs/<name>`, open the block-comment popover on its first paragraph and type a comment. */
	async function startBlockComment(name: string) {
		const tabId = mdTabsStore.add("/repo", `docs/${name}`);
		const tab = mdTabsStore.get(tabId) as FileTab;
		const { container } = render(() => <MarkdownTab tab={tab} />);
		const paragraph = await waitFor(() => {
			const el = container.querySelector<HTMLElement>("p[data-comment-source-start]");
			if (!el) throw new Error("block metadata not applied yet");
			return el;
		});
		paragraph.getBoundingClientRect = () =>
			({ left: 100, right: 500, top: 80, bottom: 120, width: 400, height: 40, x: 100, y: 80, toJSON() {} }) as DOMRect;
		fireEvent.mouseMove(paragraph.closest("#markdown-content")!.parentElement!, { clientX: 76, clientY: 96 });
		fireEvent.mouseDown(await screen.findByRole("button", { name: "Comment on this block" }));
		fireEvent.input(document.body.querySelector("textarea")!, { target: { value: "Clarify" } });
	}

	it("saves a block comment before the block the popover was opened on", async () => {
		fileContent = "# Heading\n\nOriginal paragraph.\n";
		await startBlockComment("fresh.md");

		fireEvent.click(screen.getByRole("button", { name: "Save" }));

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_file", {
				repoPath: "/repo",
				file: "docs/fresh.md",
				content: expect.stringMatching(/# Heading\n\n<!--tweak:block:\S+ @\S+\nClarify-->\nOriginal paragraph\.\n$/),
			}),
		);
	});

	// The block range comes from the render that was on screen when the popover
	// opened. If the file changes while the user types, saving at those offsets
	// would write the marker into unrelated text.
	it("refuses to save a block comment when the file changed after the popover opened", async () => {
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		fileContent = "# Heading\n\nOriginal paragraph.\n";
		await startBlockComment("stale.md");

		fileContent = "# Heading\n\nAn agent inserted this.\n\nOriginal paragraph.\n";
		repositoriesStore.bumpRevision("/repo");
		await screen.findByText("An agent inserted this.");

		fireEvent.click(screen.getByRole("button", { name: "Save" }));

		await waitFor(() =>
			expect(addToast).toHaveBeenCalledWith("Couldn't add comment", expect.stringContaining("changed"), "error"),
		);
		expect(mockInvoke).not.toHaveBeenCalledWith("write_file", expect.anything());
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
