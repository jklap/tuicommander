import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ComposePanel } from "../../components/ComposePanel/ComposePanel";
import type { QueuedCommand } from "../../hooks/usePty";
import { invoke } from "../../invoke";

vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

afterEach(async () => {
	cleanup();
	// happy-dom implements requestAnimationFrame with setImmediate. CodeMirror may
	// leave one measurement frame queued while its view is being destroyed; keep
	// that frame inside the test lifecycle so Vitest does not report an async leak.
	await new Promise<void>((resolve) => setImmediate(resolve));
	// destroy() also blurs a focused editor, and CodeMirror answers every focus
	// change with a 10ms timer — a pinned panel keeps focus after a send.
	await new Promise<void>((resolve) => setTimeout(resolve, 20));
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

function editorText(container: HTMLElement): string {
	const editor = container.querySelector(".cm-editor") as HTMLElement;
	return EditorView.findFromDOM(editor)?.state.doc.toString() ?? "";
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
		onLoadQueue: vi.fn(
			async (): Promise<QueuedCommand[]> => [
				{ id: 1, text: "run the tests", kind: "user_command" },
				{ id: 2, text: "then push", kind: "user_command" },
			],
		),
		onRemoveQueued: vi.fn(),
		pinned: () => false,
		onTogglePin: vi.fn(),
		onDismiss: vi.fn(),
		focusRequest: () => 0,
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
		// Let the send settle: a second submit while one is in flight is dropped.
		await new Promise<void>((resolve) => setImmediate(resolve));

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

	it("shows server-parked entries and lets them be removed", async () => {
		// The regression: a TUIC notice parked in the same FIFO blocked `submit`
		// with `queued_commands_pending` while this list showed nothing, so an
		// operator staring at an empty composer had nothing to act on.
		const { container, props, setQueued, getByText, getAllByTitle } = renderPanel({
			onLoadQueue: vi.fn(
				async (): Promise<QueuedCommand[]> => [
					{ id: 7, text: "[TUIC] message available", kind: "notice" },
					{ id: 8, text: "run the tests", kind: "user_command" },
				],
			),
		});
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));

		await waitFor(() => expect(container.textContent).toContain("TUIC notice"));
		expect(container.textContent).toContain("[TUIC] message available");

		fireEvent.click(getAllByTitle("Remove from queue")[0]);
		expect(props.onRemoveQueued).toHaveBeenCalledWith(7);
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

	it("closes from the close button even when pinned, where Esc only leaves the panel", async () => {
		const { container, props, getByLabelText } = renderPanel({ pinned: () => true });
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());

		fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "Escape" });
		expect(props.onDismiss).not.toHaveBeenCalled();

		fireEvent.click(getByLabelText("Close compose panel"));
		expect(props.onDismiss).toHaveBeenCalledTimes(1);
	});

	describe("pinned", () => {
		it("empties the editor after a send and stays open — it replaces the agent's input box", async () => {
			const { container, props } = renderPanel({ pinned: () => true });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "Enter", ctrlKey: true });
			expect(props.onSend).toHaveBeenCalledWith("run the tests");
			await waitFor(() => expect(editorText(container)).toBe(""));
			expect(props.onClose).not.toHaveBeenCalled();
		});

		it("empties the editor after a queue as well", async () => {
			const { container, props } = renderPanel({ pinned: () => true });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "then push");

			fireEvent.click(container.querySelector('[title^="Queue for the next idle moment"]') as HTMLElement);
			expect(props.onEnqueue).toHaveBeenCalledWith("then push");
			await waitFor(() => expect(editorText(container)).toBe(""));
		});

		it("keeps what was typed while the send was in flight — only the sent text is cleared", async () => {
			let resolveSend: () => void = () => {};
			const onSend = vi.fn(() => new Promise<void>((resolve) => (resolveSend = resolve)));
			const { container } = renderPanel({ pinned: () => true, onSend });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.keyDown(container.querySelector(".cm-content") as HTMLElement, { key: "Enter", ctrlKey: true });
			expect(onSend).toHaveBeenCalledWith("run the tests");
			// The user starts the next message while sendCommand is still writing.
			const view = EditorView.findFromDOM(container.querySelector(".cm-editor") as HTMLElement);
			view?.dispatch({ changes: { from: view.state.doc.length, insert: "\nthen push" } });

			resolveSend();
			await waitFor(() => expect(editorText(container)).toBe("then push"));
		});

		it("drops a second submit while the first is in flight, so the text is not sent twice", async () => {
			let resolveSend: () => void = () => {};
			const onSend = vi.fn(() => new Promise<void>((resolve) => (resolveSend = resolve)));
			const { container } = renderPanel({ pinned: () => true, onSend });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");
			const content = container.querySelector(".cm-content") as HTMLElement;

			fireEvent.keyDown(content, { key: "Enter", ctrlKey: true });
			fireEvent.keyDown(content, { key: "Enter", ctrlKey: true });
			fireEvent.click(container.querySelector('[title^="Queue for the next idle moment"]') as HTMLElement);
			expect(onSend).toHaveBeenCalledTimes(1);

			resolveSend();
			await waitFor(() => expect(editorText(container)).toBe(""));
			// Once settled, the panel accepts the next message.
			typeIntoEditor(container, "then push");
			fireEvent.keyDown(content, { key: "Enter", ctrlKey: true });
			expect(onSend).toHaveBeenCalledTimes(2);
			resolveSend();
			await waitFor(() => expect(editorText(container)).toBe(""));
		});

		it("keeps the text when the send fails, so nothing typed is lost", async () => {
			const onSend = vi.fn(() => Promise.reject(new Error("pty gone")));
			const { container } = renderPanel({ pinned: () => true, onSend });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.click(container.querySelector('[title^="Send"]') as HTMLElement);
			await waitFor(() => expect(onSend).toHaveBeenCalled());
			await new Promise<void>((resolve) => setImmediate(resolve));
			expect(editorText(container)).toBe("run the tests");
		});

		it("leaves the text alone when unpinned — the parent closes the panel instead", async () => {
			const { container, props } = renderPanel();
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			typeIntoEditor(container, "run the tests");

			fireEvent.click(container.querySelector('[title^="Send"]') as HTMLElement);
			expect(props.onSend).toHaveBeenCalledWith("run the tests");
			await new Promise<void>((resolve) => setImmediate(resolve));
			expect(editorText(container)).toBe("run the tests");
		});

		it("toggles through the pin button, which reports its state", async () => {
			const [pinned, setPinned] = createSignal(false);
			const onTogglePin = vi.fn(() => setPinned(!pinned()));
			const { container } = renderPanel({ pinned, onTogglePin });
			const button = () => container.querySelector("[aria-pressed]") as HTMLElement;
			await waitFor(() => expect(button()).not.toBeNull());
			expect(button().getAttribute("aria-pressed")).toBe("false");

			fireEvent.click(button());
			expect(onTogglePin).toHaveBeenCalledTimes(1);
			expect(button().getAttribute("aria-pressed")).toBe("true");
			expect(container.textContent).toContain("esc terminal");
		});

		it("moves the caret into the editor on a focus request", async () => {
			const [request, setRequest] = createSignal(0);
			const { container } = renderPanel({ pinned: () => true, focusRequest: request });
			await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
			(document.activeElement as HTMLElement | null)?.blur();
			const content = container.querySelector(".cm-content") as HTMLElement;
			await waitFor(() => expect(document.activeElement).not.toBe(content));

			setRequest(1);
			await waitFor(() => expect(document.activeElement).toBe(content));
		});
	});

	function pasteEvent(items: { type: string; file: File | null }[], text = ""): Event {
		const event = new Event("paste", { bubbles: true, cancelable: true });
		Object.defineProperty(event, "clipboardData", {
			value: {
				items: items.map((i) => ({ type: i.type, getAsFile: () => i.file })),
				getData: (type: string) => (type === "text/plain" ? text : ""),
			},
		});
		return event;
	}

	// Catches: Compose ignoring clipboard images (story 1350-a1e6) — the paste fell
	// through to CodeMirror's text paste and the image was lost.
	it("saves a pasted image and inserts its [image: path] reference", async () => {
		vi.mocked(invoke).mockResolvedValueOnce("/data/note-images/n1/pic.png");
		const { container } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		typeIntoEditor(container, "look at ");
		const content = container.querySelector(".cm-content") as HTMLElement;

		const event = pasteEvent([{ type: "image/png", file: new File(["x"], "pic.png", { type: "image/png" }) }]);
		content.dispatchEvent(event);

		expect(event.defaultPrevented).toBe(true);
		await waitFor(() => expect(editorText(container)).toContain("[image: /data/note-images/n1/pic.png]"));
		expect(invoke).toHaveBeenCalledWith(
			"save_note_image",
			expect.objectContaining({ extension: "png", dataBase64: btoa("x") }),
		);
	});

	// Catches: the image handler swallowing every paste, breaking plain text paste.
	it("leaves a text-only paste to the editor", async () => {
		vi.mocked(invoke).mockClear();
		const { container } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		const content = container.querySelector(".cm-content") as HTMLElement;

		const event = pasteEvent([{ type: "text/plain", file: null }], "plain words");
		content.dispatchEvent(event);

		expect(event.defaultPrevented).toBe(true); // CodeMirror's own text paste claims it
		expect(editorText(container)).toBe("plain words");
		expect(invoke).not.toHaveBeenCalled();
	});
});
