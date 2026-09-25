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
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { editorTabsStore } from "../../stores/editorTabs";
import { type FileTab, mdTabsStore } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { setToastBellMirrorResolver, toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";

describe("MarkdownTab agent review actions", () => {
	beforeEach(() => {
		setToastBellMirrorResolver(() => false);
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
		setToastBellMirrorResolver(() => true);
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

	it.each([
		["../src/main.rs:42", "/repo/src/main.rs", "src/main.rs", 42],
		["../LICENSE", "/repo/LICENSE", "LICENSE", undefined],
		["./My%20File.tsx#L7", "/repo/docs/My File.tsx", "docs/My File.tsx", 7],
	] as const)("opens local source link %s in the editor", async (href, absolute, relative, line) => {
		fileContent = `[link](${href})`;
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? { kind: "file", absolute_path: absolute, open_path: relative, is_directory: false, same_document: false, line }
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const open = vi.spyOn(editorTabsStore, "add").mockReturnValue("opened");
		const event = new MouseEvent("click", { bubbles: true, cancelable: true });
		link.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(true);
		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("resolve_markdown_link", {
				root: "/repo",
				currentFile: "docs/review.md",
				href,
			}),
		);
		expect(open).toHaveBeenCalledWith("/repo", relative, line, { fsRoot: "/repo" });
	});

	it("reveals a directory and toasts for a missing file", async () => {
		fileContent = "[folder](./subdir/) [missing](./gone.rs)";
		mockInvoke.mockImplementation((command: string, args: { href?: string }) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? args.href === "./gone.rs"
						? { kind: "missing", path: "./gone.rs" }
						: { kind: "file", absolute_path: "/repo/docs/subdir", open_path: "docs/subdir", is_directory: true, same_document: false }
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const links = await waitFor(() => {
			const elements = Array.from(container.querySelectorAll("a"));
			if (elements.length !== 2) throw new Error("Markdown links not rendered yet");
			return elements;
		});
		const toast = vi.spyOn(toastsStore, "add").mockReturnValue(1);
		fireEvent.click(links[0]);
		await Promise.resolve();
		expect(uiStore.state.fileBrowserExternalRoot).toBe("/repo/docs/subdir");
		expect(uiStore.state.fileBrowserPanelVisible).toBe(true);
		fireEvent.click(links[1]);
		await Promise.resolve();
		expect(toast).toHaveBeenCalledWith("File not found", "File not found: ./gone.rs", "error");
		// The File Browser visibility change persists UI prefs after a 500ms debounce.
		await new Promise((resolve) => setTimeout(resolve, 550));
	});

	it("opens a relative path outside the filesystem root", async () => {
		fileContent = "[escape](../../secret.rs)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? { kind: "file", absolute_path: "/secret.rs", open_path: "/secret.rs", is_directory: false, same_document: false }
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const open = vi.spyOn(editorTabsStore, "add").mockReturnValue("opened");
		fireEvent.click(link);
		await waitFor(() => expect(open).toHaveBeenCalledWith("/repo", "/secret.rs", undefined, { fsRoot: "/repo" }));
	});

	it("opens a symlink target outside the filesystem root", async () => {
		fileContent = "[escape](./outside.rs)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? { kind: "file", absolute_path: "/secret.rs", open_path: "/secret.rs", is_directory: false, same_document: false }
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const open = vi.spyOn(editorTabsStore, "add").mockReturnValue("opened");
		fireEvent.click(link);
		await waitFor(() =>
			expect(open).toHaveBeenCalledWith("/repo", "/secret.rs", undefined, { fsRoot: "/repo" }),
		);
	});

	it("scrolls a same-document heading without probing the filesystem", async () => {
		fileContent = "# Target Heading\n\n[go](#target-heading)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(command === "resolve_markdown_link" ? { kind: "heading", anchor: "target-heading" } : fileContent),
		);
		const scroll = vi.fn();
		const original = HTMLElement.prototype.scrollIntoView;
		HTMLElement.prototype.scrollIntoView = scroll;
		try {
			const tabId = mdTabsStore.add("/repo", "docs/review.md");
			const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
			const link = await waitFor(() => {
				const element = container.querySelector("a");
				if (!element) throw new Error("Markdown link not rendered yet");
				return element;
			});
			fireEvent.click(link);
			await waitFor(() => expect(scroll).toHaveBeenCalledWith({ block: "start" }));
			expect(mockInvoke).toHaveBeenCalledWith("resolve_markdown_link", {
				root: "/repo",
				currentFile: "docs/review.md",
				href: "#target-heading",
			});
		} finally {
			HTMLElement.prototype.scrollIntoView = original;
		}
	});

	it("scrolls a heading in a virtual plan tab", async () => {
		const provider = markdownProviderRegistry.register("test-plan", {
			provideContent: async () => "# Target Heading\n\n[go](#target-heading)",
		});
		const scroll = vi.fn();
		const original = HTMLElement.prototype.scrollIntoView;
		HTMLElement.prototype.scrollIntoView = scroll;
		try {
			const id = mdTabsStore.addVirtual("Plan", "test-plan:example");
			const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(id)!} />);
			const link = await waitFor(() => {
				const element = container.querySelector("a");
				if (!element) throw new Error("virtual Markdown link not rendered yet");
				return element;
			});
			fireEvent.click(link);
			expect(scroll).toHaveBeenCalledWith({ block: "start" });
		} finally {
			provider.dispose();
			HTMLElement.prototype.scrollIntoView = original;
		}
	});

	it("opens a linked Markdown file as a preview tab", async () => {
		fileContent = "[next](./next.md)";
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "resolve_markdown_link"
					? { kind: "file", absolute_path: "/repo/docs/next.md", open_path: "docs/next.md", is_directory: false, same_document: false }
					: fileContent,
			),
		);
		const tabId = mdTabsStore.add("/repo", "docs/review.md");
		const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
		const link = await waitFor(() => {
			const element = container.querySelector("a");
			if (!element) throw new Error("Markdown link not rendered yet");
			return element;
		});
		const add = vi.spyOn(mdTabsStore, "add").mockReturnValue("linked-tab");
		fireEvent.click(link);
		await waitFor(() => expect(add).toHaveBeenCalledWith("/repo", "docs/next.md", "/repo"));
	});

	it("scrolls to a heading after the linked Markdown tab loads", async () => {
		fileContent = "[next](./next.md#target-heading)";
		mockInvoke.mockImplementation((command: string, args: { file?: string }) => {
			if (command === "resolve_markdown_link")
				return Promise.resolve({ kind: "file", absolute_path: "/repo/docs/next.md", open_path: "docs/next.md", is_directory: false, same_document: false, anchor: "target-heading" });
			return Promise.resolve(args.file === "docs/next.md" ? "# Target Heading" : fileContent);
		});
		const scroll = vi.fn();
		const original = HTMLElement.prototype.scrollIntoView;
		HTMLElement.prototype.scrollIntoView = scroll;
		try {
			const tabId = mdTabsStore.add("/repo", "docs/review.md");
			const { container } = render(() => <MarkdownTab tab={mdTabsStore.get(tabId) as FileTab} />);
			const link = await waitFor(() => {
				const element = container.querySelector("a");
				if (!element) throw new Error("Markdown link not rendered yet");
				return element;
			});
			fireEvent.click(link);
			const next = await waitFor(() => {
				const tab = mdTabsStore.getActive();
				if (tab?.type !== "file" || tab.filePath !== "docs/next.md") throw new Error("Linked tab not open yet");
				return tab;
			});
			render(() => <MarkdownTab tab={next} />);
			await waitFor(() => expect(scroll).toHaveBeenCalledWith({ block: "start" }));
		} finally {
			HTMLElement.prototype.scrollIntoView = original;
		}
	});
});
