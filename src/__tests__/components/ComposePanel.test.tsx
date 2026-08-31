import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type ComposeAppendRequest, ComposePanel } from "../../components/ComposePanel/ComposePanel";
import { __resetModalStackForTest, pushModal } from "../../stores/modalStack";

afterEach(async () => {
	cleanup();
	// happy-dom implements requestAnimationFrame with setImmediate. CodeMirror may
	// leave one measurement frame queued while its view is being destroyed; keep
	// that frame inside the test lifecycle so Vitest does not report an async leak.
	await new Promise<void>((resolve) => setImmediate(resolve));
});

/** Type text into the panel's CodeMirror instance. Dispatching a change through
 *  the view is the only way in — the editor has no <textarea> to fill. */
function typeIntoEditor(container: HTMLElement, text: string): void {
	const editor = container.querySelector(".cm-editor") as HTMLElement | null;
	if (!editor) throw new Error("CodeMirror not mounted");
	const view = EditorView.findFromDOM(editor);
	if (!view) throw new Error("CodeMirror view not attached");
	view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

/** Read the current CodeMirror document text. */
function docText(container: HTMLElement): string {
	const editor = container.querySelector(".cm-editor") as HTMLElement | null;
	if (!editor) return "";
	const view = EditorView.findFromDOM(editor);
	return view?.state.doc.toString() ?? "";
}

/** happy-dom implements requestAnimationFrame with setImmediate — flush one. */
function flushFrame(): Promise<void> {
	return new Promise((resolve) => setImmediate(resolve));
}

function renderPanel(overrides: Partial<Parameters<typeof ComposePanel>[0]> = {}) {
	const [isOpen] = createSignal(true);
	const [queued, setQueued] = createSignal(0);
	const props = {
		isOpen,
		initialText: () => "",
		onClose: vi.fn(),
		onSend: vi.fn(),
		onEnqueue: vi.fn(),
		canEnqueue: () => true,
		queuedCount: queued,
		onClearQueue: vi.fn(),
		onLoadQueue: vi.fn(async () => [
			{ id: 1, text: "run the tests" },
			{ id: 2, text: "then push" },
		]),
		onRemoveQueued: vi.fn(),
		...overrides,
	};
	const rendered = render(() => <ComposePanel {...props} />);
	return { ...rendered, props, setQueued };
}

describe("ComposePanel", () => {
	it("Ctrl+Enter sends now, Shift+Ctrl+Enter queues instead", async () => {
		const { container, props } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "run the tests");
		const content = container.querySelector(".cm-content") as HTMLElement;

		fireEvent.keyDown(content, { key: "Enter", ctrlKey: true });
		expect(props.onSend).toHaveBeenCalledWith("run the tests");
		expect(props.onEnqueue).not.toHaveBeenCalled();

		fireEvent.keyDown(content, { key: "Enter", ctrlKey: true, shiftKey: true });
		expect(props.onEnqueue).toHaveBeenCalledWith("run the tests");
		// The immediate-send path must not fire a second time: queueing exists
		// precisely so the text does NOT reach a working agent now.
		expect(props.onSend).toHaveBeenCalledTimes(1);
	});

	it("refuses to queue for a session with no agent", async () => {
		const { container, props } = renderPanel({ canEnqueue: () => false });
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "ls -la");
		const content = container.querySelector(".cm-content") as HTMLElement;

		fireEvent.keyDown(content, { key: "Enter", ctrlKey: true, shiftKey: true });
		expect(props.onEnqueue).not.toHaveBeenCalled();
		expect(container.querySelector('[title^="Queue for the next idle moment"]')).toBeNull();
	});

	it("shows the pending count only when something is queued", async () => {
		const { container, setQueued, getByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		expect(container.textContent).not.toContain("queued");

		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
	});

	it("expands the badge into the queued texts — a count alone cannot be reviewed", async () => {
		const { container, props, setQueued, getByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		// Collapsed: no IPC round-trip for texts nobody is looking at.
		expect(props.onLoadQueue).not.toHaveBeenCalled();

		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("run the tests")).toBeTruthy());
		expect(getByText("then push")).toBeTruthy();
	});

	it("removes a single queued command by id", async () => {
		const { container, props, setQueued, getByText, getAllByTitle } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("then push")).toBeTruthy());

		fireEvent.click(getAllByTitle("Remove from queue")[1]);
		expect(props.onRemoveQueued).toHaveBeenCalledWith(2);
	});

	it("clears the whole queue from the expanded list", async () => {
		const { container, props, setQueued, getByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("Clear all")).toBeTruthy());

		fireEvent.click(getByText("Clear all"));
		expect(props.onClearQueue).toHaveBeenCalledTimes(1);
	});

	it("collapses the list when the queue drains", async () => {
		const { container, setQueued, getByText, queryByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));
		await waitFor(() => expect(getByText("run the tests")).toBeTruthy());

		setQueued(0);
		await waitFor(() => expect(queryByText("run the tests")).toBeNull());
	});

	describe("initialText seeding", () => {
		it("seeds the editor with initialText when the panel opens", async () => {
			const [isOpen, setIsOpen] = createSignal(false);
			const { container } = renderPanel({ isOpen, initialText: () => "draft from cursor line" });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			expect(docText(container)).toBe("");

			setIsOpen(true);
			await waitFor(() => expect(docText(container)).toBe("draft from cursor line"));
		});
	});

	describe("appendRequest — filling an already-open panel", () => {
		it("appends with a blank-line separator when the doc already has content", async () => {
			const [appendRequest, setAppendRequest] = createSignal<ComposeAppendRequest | null>(null);
			const { container } = renderPanel({ appendRequest });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "existing draft");

			setAppendRequest({ text: "appended prompt", seq: 1 });
			await waitFor(() => expect(docText(container)).toBe("existing draft\n\nappended prompt"));
		});

		it("appends without a separator when the doc is empty", async () => {
			const [appendRequest, setAppendRequest] = createSignal<ComposeAppendRequest | null>(null);
			const { container } = renderPanel({ appendRequest });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());

			setAppendRequest({ text: "first prompt", seq: 1 });
			await waitFor(() => expect(docText(container)).toBe("first prompt"));
		});

		it("ignores an appendRequest that is already set at mount time (defer skips the initial value)", async () => {
			const [appendRequest] = createSignal<ComposeAppendRequest | null>({ text: "stale", seq: 1 });
			const { container } = renderPanel({ appendRequest });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			await flushFrame();
			await flushFrame();

			expect(docText(container)).toBe("");
		});

		it("applies a second append after the first", async () => {
			const [appendRequest, setAppendRequest] = createSignal<ComposeAppendRequest | null>(null);
			const { container } = renderPanel({ appendRequest });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());

			setAppendRequest({ text: "first", seq: 1 });
			await waitFor(() => expect(docText(container)).toBe("first"));

			setAppendRequest({ text: "second", seq: 2 });
			await waitFor(() => expect(docText(container)).toBe("first\n\nsecond"));
		});
	});

	describe("focus reclaim (focusout handler)", () => {
		afterEach(async () => {
			__resetModalStackForTest();
			// CodeMirror's own blur observer queues an internal 10ms setTimeout
			// (updateForFocusChange) independent of anything under test here; drain
			// it so it doesn't outlive the test as an async-leak false positive.
			await new Promise((resolve) => setTimeout(resolve, 15));
		});

		// The panel's own initial-open effect (separate from the focusout handler
		// under test) unconditionally focuses the editor two animation frames
		// after mount — and `.cm-content` exists in the DOM well before that
		// chain resolves, so a test that starts driving focus right after the
		// element appears races it. Settle those two frames first so the only
		// focus change left in flight is the one each test itself makes.
		async function settleInitialMountFocus(): Promise<void> {
			await flushFrame();
			await flushFrame();
		}

		it("reclaims focus when it drops to <body>, which would otherwise silently drop keystrokes", async () => {
			const { container } = renderPanel();
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			await settleInitialMountFocus();
			const content = container.querySelector(".cm-content") as HTMLElement;
			content.focus();
			expect(document.activeElement).toBe(content);

			content.blur();
			await waitFor(() => expect(document.activeElement).toBe(content));
		});

		it("does not reclaim focus once it has moved to another element (e.g. a dialog field opened over Compose)", async () => {
			const { container } = renderPanel();
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			await settleInitialMountFocus();
			const content = container.querySelector(".cm-content") as HTMLElement;
			content.focus();

			const dialogInput = document.createElement("input");
			document.body.appendChild(dialogInput);
			content.blur();
			dialogInput.focus();

			await flushFrame();
			await flushFrame();
			expect(document.activeElement).toBe(dialogInput);

			document.body.removeChild(dialogInput);
		});

		it("does not reclaim focus while a modal is registered", async () => {
			const { container } = renderPanel();
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			await settleInitialMountFocus();
			const content = container.querySelector(".cm-content") as HTMLElement;
			content.focus();

			pushModal(() => {});
			content.blur();

			await flushFrame();
			await flushFrame();
			expect(document.activeElement).not.toBe(content);
		});
	});
});
