import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import type { CanvasTerminalRef } from "../../components/Terminal/CanvasTerminal";
import { TerminalSearch } from "../../components/Terminal/TerminalSearch";

/**
 * Regression note: the previous version of this file mocked a `CanvasTerminalRef`
 * shape (`blur`, `scrollToBottom`, `scrollLines`, `scrollToRow`, `getSelection`,
 * `clearSelection`, `selectAll`, `resize`, `getRowCount`, `getColCount`) that does
 * NOT exist on the real interface (`CanvasTerminal.tsx`'s `CanvasTerminalRef`) —
 * none of those methods are ever defined there. It also reimplemented
 * `handleSearch` locally as a call to `ref.searchFind(term)` with a single
 * argument, when the real component always calls `searchFind(term, blockScope)`.
 * Forced through `as unknown as CanvasTerminalRef`, this compiled and passed
 * while asserting nothing true about the component's actual behavior, and never
 * touched the block-scope toggle at all. This rewrite renders the real
 * `TerminalSearch` component (via @solidjs/testing-library, matching
 * SearchBar.test.tsx's pattern) against a mock built from the real ref shape.
 */

afterEach(cleanup);

/** Fixed title text SearchBar's block-scope toggle button carries — see TerminalSearch.tsx. */
const BLOCK_SCOPE_TITLE = "Search in Block (Cmd+Shift+B)";

function createMockCanvasRef(): CanvasTerminalRef {
	return {
		focus: vi.fn(),
		refresh: vi.fn(),
		resubscribe: vi.fn().mockResolvedValue(undefined),
		getSelectionText: vi.fn().mockReturnValue(""),
		searchFind: vi.fn().mockResolvedValue({ index: 0, count: 3 }),
		searchNext: vi.fn().mockReturnValue({ index: 1, count: 3 }),
		searchPrev: vi.fn().mockReturnValue({ index: 2, count: 3 }),
		searchClear: vi.fn(),
		paste: vi.fn(),
		scrollToBlock: vi.fn(),
		toggleBlockFoldAtViewport: vi.fn(),
	};
}

describe("TerminalSearch", () => {
	it("calls searchFind with blockScope=false on the initial search", async () => {
		const canvasRef = createMockCanvasRef();
		const { getByPlaceholderText } = render(() => (
			<TerminalSearch visible={true} canvasRef={canvasRef} onClose={() => {}} />
		));

		const input = getByPlaceholderText("Find…") as HTMLInputElement;
		await waitFor(() => expect(document.activeElement).toBe(input));
		fireEvent.input(input, { target: { value: "hello" } });

		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenCalledWith("hello", false));
	});

	it("re-issues the same term with blockScope=true when the block-scope toggle is turned on", async () => {
		const canvasRef = createMockCanvasRef();
		const { getByPlaceholderText, getByTitle } = render(() => (
			<TerminalSearch visible={true} canvasRef={canvasRef} onClose={() => {}} />
		));

		const input = getByPlaceholderText("Find…") as HTMLInputElement;
		await waitFor(() => expect(document.activeElement).toBe(input));
		fireEvent.input(input, { target: { value: "hello" } });
		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenCalledWith("hello", false));

		// Re-query the toggle after every click: flipping `blockScope()` re-runs the
		// `<For each={props.extraToggles}>` in SearchBar with a brand-new array/object
		// literal (Solid re-evaluates the prop getter), so `<For>`'s reference-identity
		// diffing tears down and recreates the button element — the node captured
		// before the click is stale afterward (same class of gotcha as `<For>` index
		// staleness, but by node identity instead of by index).
		fireEvent.click(getByTitle(BLOCK_SCOPE_TITLE));

		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenLastCalledWith("hello", true));
	});

	it("re-issues the same term with blockScope=false when the toggle is turned back off", async () => {
		const canvasRef = createMockCanvasRef();
		const { getByPlaceholderText, getByTitle } = render(() => (
			<TerminalSearch visible={true} canvasRef={canvasRef} onClose={() => {}} />
		));

		const input = getByPlaceholderText("Find…") as HTMLInputElement;
		await waitFor(() => expect(document.activeElement).toBe(input));
		fireEvent.input(input, { target: { value: "hello" } });
		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenCalledWith("hello", false));

		fireEvent.click(getByTitle(BLOCK_SCOPE_TITLE)); // on
		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenLastCalledWith("hello", true));

		fireEvent.click(getByTitle(BLOCK_SCOPE_TITLE)); // off — re-queried, see note above
		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenLastCalledWith("hello", false));
	});

	it("does not call searchFind when the toggle is flipped before any term was searched", async () => {
		const canvasRef = createMockCanvasRef();
		const { getByPlaceholderText, getByTitle } = render(() => (
			<TerminalSearch visible={true} canvasRef={canvasRef} onClose={() => {}} />
		));
		await waitFor(() => expect(document.activeElement).toBe(getByPlaceholderText("Find…")));

		fireEvent.click(getByTitle(BLOCK_SCOPE_TITLE));

		expect(canvasRef.searchFind).not.toHaveBeenCalled();
	});

	it("clearing the term calls searchClear instead of searchFind", async () => {
		const canvasRef = createMockCanvasRef();
		const { getByPlaceholderText } = render(() => (
			<TerminalSearch visible={true} canvasRef={canvasRef} onClose={() => {}} />
		));

		const input = getByPlaceholderText("Find…") as HTMLInputElement;
		await waitFor(() => expect(document.activeElement).toBe(input));
		fireEvent.input(input, { target: { value: "hello" } });
		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenCalledTimes(1));

		fireEvent.input(input, { target: { value: "" } });
		await waitFor(() => expect(canvasRef.searchClear).toHaveBeenCalled());
		expect(canvasRef.searchFind).toHaveBeenCalledTimes(1);
	});

	it("clears search state via the ref when the bar becomes hidden", async () => {
		const canvasRef = createMockCanvasRef();
		render(() => <TerminalSearch visible={false} canvasRef={canvasRef} onClose={() => {}} />);
		await waitFor(() => expect(canvasRef.searchClear).toHaveBeenCalled());
	});

	it("searchNext and searchPrev delegate to the ref", async () => {
		const canvasRef = createMockCanvasRef();
		const { getByPlaceholderText, getByTitle } = render(() => (
			<TerminalSearch visible={true} canvasRef={canvasRef} onClose={() => {}} />
		));

		const input = getByPlaceholderText("Find…") as HTMLInputElement;
		await waitFor(() => expect(document.activeElement).toBe(input));
		fireEvent.input(input, { target: { value: "hello" } });
		await waitFor(() => expect(canvasRef.searchFind).toHaveBeenCalledTimes(1));

		fireEvent.click(getByTitle("Next Match (Enter)"));
		expect(canvasRef.searchNext).toHaveBeenCalledOnce();

		fireEvent.click(getByTitle("Previous Match (Shift+Enter)"));
		expect(canvasRef.searchPrev).toHaveBeenCalledOnce();
	});

	it("searchNext/searchPrev are no-ops before any term has been searched", async () => {
		const canvasRef = createMockCanvasRef();
		const { getByPlaceholderText, getByTitle } = render(() => (
			<TerminalSearch visible={true} canvasRef={canvasRef} onClose={() => {}} />
		));
		await waitFor(() => expect(document.activeElement).toBe(getByPlaceholderText("Find…")));

		fireEvent.click(getByTitle("Next Match (Enter)"));
		fireEvent.click(getByTitle("Previous Match (Shift+Enter)"));

		expect(canvasRef.searchNext).not.toHaveBeenCalled();
		expect(canvasRef.searchPrev).not.toHaveBeenCalled();
	});
});
