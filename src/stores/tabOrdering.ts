import { createStore, produce } from "solid-js/store";
import { orderedThenRemainder, reorderIds } from "./tabManager";

interface TabOrderingState {
	order: string[];
}

// DEFERRED (2026-09-05) — `insert` and `remove` have no production caller, so
// `order` is permanently empty: `reorder` finds neither id and returns, and
// `getOrdered` degenerates to the caller's set-insertion order. Free-mode drag
// across tab kinds therefore does nothing. Populating `order` means calling
// insert/remove from the add/close paths of terminals, diff, md and editor tabs,
// and the drop handler in TabBar is drag & drop code, which needs Boss's approval
// (CLAUDE.md "Drag & Drop"). Story 682-b8d2 stopped here.
function createTabOrderingStore() {
	const [state, setState] = createStore<TabOrderingState>({ order: [] });

	function insert(id: string, afterId?: string): void {
		if (state.order.includes(id)) return;
		setState(
			produce((s) => {
				if (afterId) {
					const idx = s.order.indexOf(afterId);
					if (idx !== -1) {
						s.order.splice(idx + 1, 0, id);
						return;
					}
				}
				s.order.push(id);
			}),
		);
	}

	function remove(id: string): void {
		setState(
			produce((s) => {
				const idx = s.order.indexOf(id);
				if (idx !== -1) s.order.splice(idx, 1);
			}),
		);
	}

	function reorder(sourceId: string, targetId: string, side: "before" | "after"): void {
		if (sourceId === targetId) return;
		setState(
			produce((s) => {
				reorderIds(s.order, sourceId, targetId, side);
			}),
		);
	}

	function getOrdered(visibleIds: Set<string>): string[] {
		return orderedThenRemainder(state.order, visibleIds, (id) => visibleIds.has(id));
	}

	function clear(): void {
		setState("order", []);
	}

	return { state, insert, remove, reorder, getOrdered, clear };
}

export const tabOrderingStore = createTabOrderingStore();
