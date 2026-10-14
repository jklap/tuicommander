import { expect, it, vi } from "vitest";
import { allDemoRows, listDemo } from "../components/Tasks/list/demo";
import { initialTaskQuery } from "../components/Tasks/list/useTasksListQuery";
import { collectLocalTasks } from "./tasksLocalQuery";

// Catches: provider rejects local title sort because collection forwards it, or stops at page one.
it("collects_120_items_without_forwarding_local_query_operations", async () => {
	const provider = listDemo(120);
	const list = provider.list.bind(provider);
	provider.list = vi.fn(async (query, signal) => {
		expect(query.sort).toBeNull();
		expect(query.groupBy).toBeNull();
		expect(query.filters).toEqual([]);
		return { ...(await list(query, signal)), total: 120 };
	});
	const items = await collectLocalTasks(
		provider,
		{ ...initialTaskQuery(), sort: { fieldId: "title", descending: true } },
		new AbortController().signal,
	);
	expect(items).toHaveLength(120);
	expect(provider.list).toHaveBeenCalledTimes(2);
});

// Catches: changed totals, duplicate-only pages, or a repeating cursor certify a partial list as complete.
it("refuses_inconsistent_or_nonterminal_collection", async () => {
	const seed = listDemo(2);
	const rows = await allDemoRows(seed);
	for (const scenario of ["changed", "repeat", "missing", "overfull"]) {
		let calls = 0;
		const provider = {
			...seed,
			list: async () => {
				calls++;
				if (scenario === "overfull") return { items: rows, total: 1, nextCursor: null };
				if (calls === 1) return { items: [rows[0]], total: 2, nextCursor: "next" };
				return {
					items: [rows[0]],
					total: scenario === "changed" ? 3 : 2,
					nextCursor: scenario === "repeat" ? "next" : null,
				};
			},
		};
		expect(await collectLocalTasks(provider, initialTaskQuery(), new AbortController().signal)).toBeNull();
		expect(calls).toBeLessThanOrEqual(2);
	}
});

// Catches: unknown totals are invented, or a large list triggers automatic full loading.
it("unknown_or_large_totals_keep_server_mode", async () => {
	const seed = listDemo(1);
	for (const total of [undefined, 1001]) {
		const list = vi.fn(async () => ({ items: [], total, nextCursor: "next" }));
		expect(await collectLocalTasks({ ...seed, list }, initialTaskQuery(), new AbortController().signal)).toBeNull();
		expect(list).toHaveBeenCalledTimes(1);
	}
});
