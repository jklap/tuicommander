import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ComposePanel } from "../../components/ComposePanel/ComposePanel";

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

	it("shows the pending count only when something is queued, and clears it on click", async () => {
		const { container, props, setQueued, getByText } = renderPanel();
		await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());
		expect(container.textContent).not.toContain("queued");

		setQueued(2);
		await waitFor(() => expect(getByText(/2 queued/)).toBeTruthy());
		fireEvent.click(getByText(/2 queued/));
		expect(props.onClearQueue).toHaveBeenCalledTimes(1);
	});
});
