import { createSignal } from "solid-js";
import type { WorkItemProvider } from "../plugins/workItemProvider";
import type {
	WorkItem,
	WorkItemGroup,
	WorkItemGroupedPage,
	WorkItemPage,
	WorkItemQuery,
} from "../types/work-items.gen";

type Source = Pick<WorkItemProvider, "providerId" | "accountId" | "list" | "grouped">;
interface QueryState {
	items: WorkItem[];
	groups: WorkItemGroup[];
	nextCursor: string | null;
	loading: boolean;
	stale: boolean;
	error: string | null;
}
const empty = (): QueryState => ({
	items: [],
	groups: [],
	nextCursor: null,
	loading: false,
	stale: false,
	error: null,
});

/** View state only: provider-side filtering, sorting and pagination remain authoritative. */
export function createTasksQuery(options: { maxCachedPages?: number } = {}) {
	const requestedCapacity = options.maxCachedPages ?? 32;
	const capacity = Number.isFinite(requestedCapacity) ? Math.max(1, Math.floor(requestedCapacity)) : 32;
	const cache = new Map<string, WorkItemPage | WorkItemGroupedPage>();
	const [state, setState] = createSignal<QueryState>(empty());
	let generation = 0;
	let source: Source | undefined;
	let query: WorkItemQuery | undefined;
	let acceptedQuery: WorkItemQuery | undefined;
	let controller: AbortController | undefined;
	const consumedCursors = new Set<string>();
	const key = (provider: Source, request: WorkItemQuery) =>
		JSON.stringify([provider.providerId, provider.accountId, request]);
	const identity = (item: WorkItem) =>
		JSON.stringify([item.reference.providerId, item.reference.accountId, item.reference.id]);

	async function request(append: boolean, force: boolean) {
		if (!source || !query) return;
		const provider = source;
		const token = generation;
		const cursor = append ? state().nextCursor : null;
		if (append && (!cursor || state().loading || consumedCursors.has(cursor))) return;
		const pageQuery = { ...query, cursor };
		const pageKey = key(provider, pageQuery);
		controller?.abort();
		const abort = new AbortController();
		controller = abort;
		setState((previous) => ({ ...previous, loading: true, error: null }));
		try {
			let page = force ? undefined : cache.get(pageKey);
			if (!page)
				page = pageQuery.groupBy
					? await provider.grouped(pageQuery, abort.signal)
					: await provider.list(pageQuery, abort.signal);
			if (token !== generation || abort.signal.aborted) return;
			cache.delete(pageKey);
			cache.set(pageKey, page);
			while (cache.size > capacity) {
				const oldest = cache.keys().next().value;
				if (oldest === undefined) break;
				cache.delete(oldest);
			}
			acceptedQuery = structuredClone({ ...pageQuery, cursor: null });
			if (cursor) consumedCursors.add(cursor);
			const rows = new Map<string, WorkItem>();
			if (append) for (const item of state().items) rows.set(identity(item), item);
			const groups = new Map<string, WorkItemGroup>();
			if ("groups" in page) {
				if (append) for (const group of state().groups) groups.set(group.id, group);
				for (const group of page.groups) {
					const groupedRows = new Map((groups.get(group.id)?.items ?? []).map((item) => [identity(item), item]));
					for (const item of group.items) {
						groupedRows.set(identity(item), item);
						rows.set(identity(item), item);
					}
					groups.set(group.id, { ...group, items: [...groupedRows.values()] });
				}
			} else for (const item of page.items) rows.set(identity(item), item);
			setState({
				items: [...rows.values()],
				groups: [...groups.values()],
				nextCursor: page.nextCursor && !consumedCursors.has(page.nextCursor) ? page.nextCursor : null,
				loading: false,
				stale: false,
				error: null,
			});
		} catch (error) {
			if (token !== generation || abort.signal.aborted) return;
			query = acceptedQuery ?? query;
			setState((previous) => ({
				...previous,
				loading: false,
				stale: previous.items.length > 0,
				error: error instanceof Error ? error.message : String(error),
			}));
		}
	}

	return {
		state,
		acceptedQuery: () => acceptedQuery,
		async select(provider: Source, nextQuery: WorkItemQuery) {
			const sameSource = source?.providerId === provider.providerId && source?.accountId === provider.accountId;
			generation++;
			controller?.abort();
			if (!sameSource) acceptedQuery = undefined;
			source = provider;
			query = structuredClone({ ...nextQuery, cursor: null });
			consumedCursors.clear();
			setState((previous) =>
				sameSource ? { ...previous, nextCursor: null, stale: previous.items.length > 0 } : empty(),
			);
			await request(false, false);
		},
		loadMore: () => request(true, false),
		async refresh() {
			generation++;
			cache.clear();
			consumedCursors.clear();
			await request(false, true);
		},
		clear() {
			generation++;
			controller?.abort();
			source = undefined;
			query = undefined;
			acceptedQuery = undefined;
			consumedCursors.clear();
			setState(empty());
		},
		dispose() {
			generation++;
			controller?.abort();
			cache.clear();
		},
	};
}
