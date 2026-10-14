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

// Catches: local collection clears the MR discriminator and certifies an issue-only scope instead.
it("local_collection_preserves_gitlab_merge_request_scope", async () => {
	const { createGitLabProvider } = await import("../../plugins/gitlab/provider.js");
	const { default: recorded } = await import("../../plugins/gitlab/fixtures/recorded.json");
	const provider = createGitLabProvider({
		account: { id: "local", host: "https://gitlab.example.invalid:8443", login: "root" },
		project: "review/demo",
		fetch: async (request: { url: string }) => ({
			status: 200,
			headers: { "x-total": "1" },
			json: [request.url.includes("/merge_requests?") ? recorded.mr : recorded.issue],
		}),
	});
	const rows = await collectLocalTasks(
		provider,
		{
			filters: [{ fieldId: "type", values: [{ kind: "enum", value: "merge_request" }] }],
			savedFilterId: null,
			search: "",
			sort: null,
			groupBy: null,
			cursor: null,
			limit: 100,
		},
		new AbortController().signal,
	);
	expect(rows?.map((item) => item.itemType)).toEqual(["merge_request"]);
});

// Catches: state=all becomes the default opened scope, or an MR preset loses its endpoint.
it("gitlab_collection_preserves_state_and_preset_scope", async () => {
	const { createGitLabProvider } = await import("../../plugins/gitlab/provider.js");
	const { default: recorded } = await import("../../plugins/gitlab/fixtures/recorded.json");
	const paths: URL[] = [];
	const provider = createGitLabProvider({
		account: { id: "local", host: "https://gitlab.example.invalid:8443", login: "root" },
		fetch: async (request: { url: string }) => {
			paths.push(new URL(request.url));
			return { status: 200, headers: { "x-total": "1" }, json: [recorded.mr] };
		},
	});
	const selected = provider.savedFilters.find((preset) => preset.id === "assigned-mrs");
	expect(selected).toBeDefined();
	await collectLocalTasks(provider, selected!.query, new AbortController().signal);
	expect(paths[0].pathname).toMatch(/merge_requests$/);
	expect(paths[0].searchParams.get("scope")).toBe("assigned_to_me");
	expect(paths[0].searchParams.get("state")).toBe("opened");
	await collectLocalTasks(
		provider,
		{
			...selected!.query,
			savedFilterId: null,
			filters: [
				{ fieldId: "type", values: [{ kind: "enum", value: "merge_request" }] },
				{ fieldId: "state", values: [{ kind: "enum", value: "all" }] },
			],
		},
		new AbortController().signal,
	);
	expect(paths[1].searchParams.has("state")).toBe(false);
});
