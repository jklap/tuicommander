import { createSignal } from "solid-js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import type { WorkItemBoardPage, WorkItemQuery } from "../../../types/work-items.gen";
import { cardKey } from "./types";

const query: WorkItemQuery = {
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 50,
};
/** View pagination only; columns, placement and move legality come from the provider. */
export function createBoardPages() {
	const [page, setPage] = createSignal<WorkItemBoardPage>();
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal("");
	let source: WorkItemProvider | undefined;
	let boardId = "";
	let generation = 0;
	let controller: AbortController | undefined;
	let consumed = new Set<string>();
	let depth = 1;
	function cancel() {
		generation++;
		controller?.abort();
		setLoading(false);
	}
	function merge(previous: WorkItemBoardPage | undefined, next: WorkItemBoardPage): WorkItemBoardPage {
		const cards = new Map<string, { columnId: string; card: WorkItemBoardPage["lanes"][number]["cards"][number] }>();
		for (const lane of [...(previous?.lanes ?? []), ...next.lanes])
			for (const card of lane.cards) cards.set(cardKey(card.item.reference), { columnId: lane.columnId, card });
		const lanes = new Map(
			next.board.columns.map((column) => [
				column.id,
				{ columnId: column.id, cards: [] as WorkItemBoardPage["lanes"][number]["cards"] },
			]),
		);
		for (const { columnId, card } of cards.values()) lanes.get(columnId)?.cards.push(card);
		return {
			...next,
			lanes: [...lanes.values()],
			nextCursor: next.nextCursor && !consumed.has(next.nextCursor) ? next.nextCursor : null,
		};
	}
	async function request(append: boolean) {
		const provider = source;
		const id = boardId;
		const cursor = append ? (page()?.nextCursor ?? null) : null;
		if (!provider || !id || (append && (!cursor || loading()))) return;
		cancel();
		const attempt = generation;
		const abort = new AbortController();
		controller = abort;
		setLoading(true);
		setError("");
		const previousConsumed = consumed;
		consumed = append ? new Set(consumed) : new Set();
		try {
			let next = await provider.board(id, { ...query, cursor }, abort.signal);
			if (attempt !== generation || abort.signal.aborted) return;
			if (cursor) consumed.add(cursor);
			let snapshot = merge(append ? page() : undefined, next);
			// Re-fetch the loaded depth after a move, so refresh does not collapse the board to page one.
			if (!append)
				for (let index = 1; index < depth && snapshot.nextCursor; index++) {
					const following = snapshot.nextCursor;
					next = await provider.board(id, { ...query, cursor: following }, abort.signal);
					if (attempt !== generation || abort.signal.aborted) return;
					consumed.add(following);
					snapshot = merge(snapshot, next);
				}
			if (append) depth++;
			setPage(snapshot);
		} catch (cause) {
			if (attempt !== generation || abort.signal.aborted) return;
			consumed = previousConsumed;
			setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			if (attempt === generation) setLoading(false);
		}
	}
	return {
		page,
		loading,
		error,
		cancel,
		select(provider: WorkItemProvider, id: string) {
			cancel();
			source = provider;
			boardId = id;
			depth = 1;
			consumed = new Set();
			setPage(undefined);
			setError("");
			if (id) void request(false);
		},
		more: () => request(true),
		refresh: () => request(false),
	};
}
