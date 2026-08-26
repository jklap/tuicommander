import { type CellMetrics, GUTTER_PX } from "./canvasTerminalUtils";

export function canvasToGrid(
	metrics: () => CellMetrics | null,
	canvasRef: HTMLCanvasElement,
	e: MouseEvent,
	cachedRect?: DOMRect,
): { col: number; row: number } {
	const m = metrics();
	if (!m) return { col: 0, row: 0 };
	const rect = cachedRect ?? canvasRef.getBoundingClientRect();
	const x = e.clientX - rect.left - GUTTER_PX;
	const y = e.clientY - rect.top;
	const maxCol = Math.max(0, Math.floor((rect.width - GUTTER_PX) / m.cellWidth) - 1);
	const maxRow = Math.max(0, Math.floor(rect.height / m.cellHeight) - 1);
	return {
		col: Math.max(0, Math.min(Math.floor(x / m.cellWidth), maxCol)),
		row: Math.max(0, Math.min(Math.floor(y / m.cellHeight), maxRow)),
	};
}

/** Last selectable column of a line — shared by triple-click and line-mode
 *  drag extension so both agree with canvasToGrid's own maxCol (which
 *  subtracts GUTTER_PX; this used to be computed ad hoc without it). */
export function lastGridCol(metrics: () => CellMetrics | null, rect: DOMRect): number {
	const m = metrics();
	if (!m) return 79;
	return Math.max(0, Math.floor((rect.width - GUTTER_PX) / m.cellWidth) - 1);
}

export function mouseModifiers(e: MouseEvent): number {
	return (e.shiftKey ? 4 : 0) | (e.altKey ? 8 : 0) | (e.ctrlKey ? 16 : 0);
}

export function sgrMouseSequence(button: number, col: number, row: number, press: boolean, e?: MouseEvent): string {
	const cb = button + (e ? mouseModifiers(e) : 0);
	return `\x1b[<${cb};${col + 1};${row + 1}${press ? "M" : "m"}`;
}
