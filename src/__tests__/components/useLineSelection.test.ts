import { createRoot } from "solid-js";
import { describe, expect, it } from "vitest";
import { createLineSelection } from "../../components/DiffTab/useLineSelection";

const SELECTED_CLASS = "line-selected";

/** One hunk's worth of unified-diff text, matching what `extractHunks()`
 *  would produce: a `@@ ... @@` header followed by +/-/space body lines. */
function hunkText(bodyLines: string[]): string {
	return ["@@ -1,3 +1,3 @@", ...bodyLines].join("\n");
}

/** Build a fake rendered-diff DOM: one `<tr>` per row, a hunk-header row
 *  (matched via a descendant `[class*='diff-line-hunk']`) followed by rows
 *  carrying `data-line-old-num`/`data-line-new-num` the way @git-diff-view
 *  marks deletions/additions/context. */
function buildTable(hunks: Array<Array<"add" | "del" | "context">>): {
	container: HTMLElement;
	rows: HTMLTableRowElement[][];
} {
	const container = document.createElement("div");
	const table = document.createElement("table");
	const tbody = document.createElement("tbody");
	const rows: HTMLTableRowElement[][] = [];

	for (const hunk of hunks) {
		const headerRow = document.createElement("tr");
		const marker = document.createElement("td");
		marker.className = "diff-line-hunk-content";
		headerRow.appendChild(marker);
		tbody.appendChild(headerRow);

		const hunkRows: HTMLTableRowElement[] = [];
		for (const kind of hunk) {
			const row = document.createElement("tr");
			const cell = document.createElement("td");
			if (kind === "add") cell.setAttribute("data-line-new-num", "1");
			else if (kind === "del") cell.setAttribute("data-line-old-num", "1");
			else {
				cell.setAttribute("data-line-new-num", "1");
				cell.setAttribute("data-line-old-num", "1");
			}
			row.appendChild(cell);
			tbody.appendChild(row);
			hunkRows.push(row);
		}
		rows.push(hunkRows);
	}

	table.appendChild(tbody);
	container.appendChild(table);
	document.body.appendChild(container);
	return { container, rows };
}

function mouseEvent(target: Element, button = 0): MouseEvent {
	return { target, button, preventDefault: () => {} } as unknown as MouseEvent;
}

describe("createLineSelection", () => {
	it("mousedown on a change row selects that one line", () => {
		createRoot((dispose) => {
			const { container, rows } = buildTable([["del", "add"]]);
			const hunkStr = hunkText(["-old", "+new"]);
			const sel = createLineSelection({ hunks: () => [hunkStr], selectedClass: SELECTED_CLASS });
			sel.setContentRef(container);

			sel.handlers.onMouseDown(mouseEvent(rows[0][0].firstElementChild as Element));
			sel.handlers.onMouseUp();

			expect(sel.selectedHunkIdx()).toBe(0);
			expect(sel.selectedLines()).toEqual(new Set([0]));
			dispose();
			container.remove();
		});
	});

	it("dragging within the same hunk extends the selection", () => {
		createRoot((dispose) => {
			const { container, rows } = buildTable([["del", "add"]]);
			const hunkStr = hunkText(["-old", "+new"]);
			const sel = createLineSelection({ hunks: () => [hunkStr], selectedClass: SELECTED_CLASS });
			sel.setContentRef(container);

			sel.handlers.onMouseDown(mouseEvent(rows[0][0].firstElementChild as Element));
			sel.handlers.onMouseMove(mouseEvent(rows[0][1].firstElementChild as Element));
			sel.handlers.onMouseUp();

			expect(sel.selectedLines()).toEqual(new Set([0, 1]));
			dispose();
			container.remove();
		});
	});

	it("dragging into a different hunk is ignored", () => {
		createRoot((dispose) => {
			const { container, rows } = buildTable([["del"], ["add"]]);
			const hunks = [hunkText(["-old"]), hunkText(["+new"])];
			const sel = createLineSelection({ hunks: () => hunks, selectedClass: SELECTED_CLASS });
			sel.setContentRef(container);

			sel.handlers.onMouseDown(mouseEvent(rows[0][0].firstElementChild as Element));
			// Move into the second hunk's row — must not extend/relocate selection.
			sel.handlers.onMouseMove(mouseEvent(rows[1][0].firstElementChild as Element));

			expect(sel.selectedHunkIdx()).toBe(0);
			expect(sel.selectedLines()).toEqual(new Set([0]));
			dispose();
			container.remove();
		});
	});

	it("context lines (both old and new attrs) never select", () => {
		createRoot((dispose) => {
			const { container, rows } = buildTable([["context", "add"]]);
			const hunkStr = hunkText([" ctx", "+new"]);
			const sel = createLineSelection({ hunks: () => [hunkStr], selectedClass: SELECTED_CLASS });
			sel.setContentRef(container);

			sel.handlers.onMouseDown(mouseEvent(rows[0][0].firstElementChild as Element));

			expect(sel.selectedHunkIdx()).toBeNull();
			expect(sel.selectedLines().size).toBe(0);
			dispose();
			container.remove();
		});
	});

	it("clear() empties the selection and strips the selected class", async () => {
		await createRoot(async (dispose) => {
			const { container, rows } = buildTable([["del", "add"]]);
			const hunkStr = hunkText(["-old", "+new"]);
			const sel = createLineSelection({ hunks: () => [hunkStr], selectedClass: SELECTED_CLASS });
			sel.setContentRef(container);

			sel.handlers.onMouseDown(mouseEvent(rows[0][0].firstElementChild as Element));
			// Let the RAF-scheduled restyle effect run.
			await new Promise((r) => requestAnimationFrame(r));
			expect(rows[0][0].classList.contains(SELECTED_CLASS)).toBe(true);

			sel.clear();
			expect(sel.selectedLines().size).toBe(0);
			expect(sel.selectedHunkIdx()).toBeNull();
			expect(container.querySelectorAll(`.${SELECTED_CLASS}`).length).toBe(0);
			dispose();
			container.remove();
		});
	});

	it("invalidate() forces the row cache to rebuild against new DOM", () => {
		createRoot((dispose) => {
			const { container, rows } = buildTable([["del", "add"]]);
			const hunkStr = hunkText(["-old", "+new"]);
			const sel = createLineSelection({ hunks: () => [hunkStr], selectedClass: SELECTED_CLASS });
			sel.setContentRef(container);
			sel.handlers.onMouseDown(mouseEvent(rows[0][0].firstElementChild as Element));
			sel.handlers.onMouseUp();
			expect(sel.selectedLines()).toEqual(new Set([0]));

			// Replace the DOM entirely (as a fresh diff render would) and
			// invalidate — the stale row cache must not be reused.
			container.innerHTML = "";
			const { rows: newRows } = buildTable([["add", "del"]]);
			container.appendChild(newRows[0][0].closest("table")!);
			sel.invalidate();

			sel.handlers.onMouseDown(mouseEvent(newRows[0][1].firstElementChild as Element));
			expect(sel.selectedLines()).toEqual(new Set([1]));
			dispose();
			container.remove();
		});
	});

	it("ignoreSelector prevents starting a drag from within it (e.g. a revert button)", () => {
		createRoot((dispose) => {
			const { container, rows } = buildTable([["del", "add"]]);
			const hunkStr = hunkText(["-old", "+new"]);
			const sel = createLineSelection({
				hunks: () => [hunkStr],
				selectedClass: SELECTED_CLASS,
				ignoreSelector: ".revert-btn",
			});
			sel.setContentRef(container);

			const btn = document.createElement("button");
			btn.className = "revert-btn";
			rows[0][0].firstElementChild!.appendChild(btn);

			sel.handlers.onMouseDown(mouseEvent(btn));
			expect(sel.selectedHunkIdx()).toBeNull();
			dispose();
			container.remove();
		});
	});
});
