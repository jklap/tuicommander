import { createEffect, createMemo, createSignal, For, onCleanup, Show, untrack } from "solid-js";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import { createTasksQuery } from "../../../stores/tasksQuery";
import type {
	WorkItemFieldSchema,
	WorkItemFieldValue,
	WorkItemQuery,
	WorkItemRef,
} from "../../../types/work-items.gen";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { TaskRows } from "./TaskRows";
import s from "./tasksList.module.css";

export interface TasksListProps {
	providers: readonly WorkItemProvider[];
	onOpen: (reference: WorkItemRef) => void;
	visible?: boolean;
}
const initialQuery = (): WorkItemQuery => ({
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 100,
});
const sourceKey = (provider: WorkItemProvider) => JSON.stringify([provider.providerId, provider.accountId]);

function filterValue(field: WorkItemFieldSchema, value: string): WorkItemFieldValue | undefined {
	switch (field.kind) {
		case "text":
		case "markdown":
		case "date":
		case "link":
		case "enum":
			return { kind: field.kind, value };
		case "number":
			return Number.isFinite(Number(value)) ? { kind: "number", value: Number(value) } : undefined;
		case "labels":
			return {
				kind: "labels",
				value: [{ id: value, name: field.options.find((option) => option.id === value)?.name ?? value }],
			};
		case "user":
			return {
				kind: "user",
				value: [
					{ id: value, name: field.options.find((option) => option.id === value)?.name ?? value, avatarUrl: null },
				],
			};
	}
}

/** Generic host-owned list; provider descriptors and query results supply all item data. */
export function TasksList(props: TasksListProps) {
	const model = createTasksQuery();
	const [selectedSource, setSelectedSource] = createSignal("");
	const provider = createMemo(
		() => props.providers.find((entry) => sourceKey(entry) === selectedSource()) ?? props.providers[0],
	);
	const selectedKey = () => {
		const current = provider();
		return current ? sourceKey(current) : "";
	};
	const [query, setQuery] = createSignal(initialQuery());
	const [search, setSearch] = createSignal("");
	let debounce: ReturnType<typeof setTimeout> | undefined;
	let restoredQuery: WorkItemQuery | undefined;
	let previousSource: WorkItemProvider | undefined;
	createEffect(() => {
		const current = provider();
		if (current !== previousSource) {
			previousSource = current;
			clearTimeout(debounce);
			setSearch("");
			setQuery(initialQuery());
		}
	});
	createEffect(() => {
		const current = provider();
		const request = query();
		if (props.visible === false) return;
		untrack(() => {
			if (request === restoredQuery) {
				restoredQuery = undefined;
				return;
			}
			if (current)
				void model.select(current, request).then(() => {
					if (current !== provider() || request !== query() || !model.state().error) return;
					restoredQuery = structuredClone(model.acceptedQuery() ?? initialQuery());
					setSearch(restoredQuery.search);
					setQuery(restoredQuery);
				});
			else model.clear();
		});
	});
	onCleanup(() => {
		clearTimeout(debounce);
		model.dispose();
	});
	function updateSearch(value: string) {
		setSearch(value);
		clearTimeout(debounce);
		debounce = setTimeout(() => setQuery((previous) => ({ ...previous, savedFilterId: null, search: value })), 250);
	}
	function applyPreset(id: string) {
		clearTimeout(debounce);
		const preset = provider()?.savedFilters.find((entry) => entry.id === id);
		const next = preset ? structuredClone({ ...preset.query, savedFilterId: id, cursor: null }) : initialQuery();
		setSearch(next.search);
		setQuery(next);
	}
	function applyFilter(field: WorkItemFieldSchema, raw: string) {
		const value = raw ? filterValue(field, raw) : undefined;
		setQuery((previous) => ({
			...previous,
			savedFilterId: null,
			filters: [
				...previous.filters.filter((entry) => entry.fieldId !== field.id),
				...(value ? [{ fieldId: field.id, values: [value] }] : []),
			],
		}));
	}
	function selectedFilter(id: string) {
		const value = query().filters.find((entry) => entry.fieldId === id)?.values[0];
		if (!value) return "";
		if (value.kind === "user" || value.kind === "labels") return value.value[0]?.id ?? "";
		return String(value.value);
	}

	return (
		<section class={s.list} aria-label="Tasks">
			<div class={s.toolbar}>
				<label>
					Source
					<Select
						aria-label="Task source"
						value={selectedKey()}
						onChange={(event) => setSelectedSource(event.currentTarget.value)}
					>
						<For each={props.providers}>
							{(entry) => (
								<option value={sourceKey(entry)}>
									{entry.name} · {entry.accountId}
								</option>
							)}
						</For>
					</Select>
				</label>
				<label>
					Preset
					<Select
						aria-label="Task preset"
						value={query().savedFilterId ?? ""}
						onChange={(event) => applyPreset(event.currentTarget.value)}
					>
						<option value="">All items</option>
						<For each={provider()?.savedFilters}>{(entry) => <option value={entry.id}>{entry.name}</option>}</For>
					</Select>
				</label>
				<label class={s.search}>
					Search
					<Input
						type="search"
						aria-label="Search tasks"
						value={search()}
						onInput={(event) => updateSearch(event.currentTarget.value)}
					/>
				</label>
				<button
					type="button"
					disabled={!provider() || model.state().loading || props.visible === false}
					onClick={() => void model.refresh()}
				>
					Refresh
				</button>
			</div>
			<Show when={provider()}>
				{(current) => (
					<div class={s.filters}>
						<For each={current().fields.filter((field) => field.filterable)}>
							{(field) => (
								<label>
									{field.name}
									<Show
										when={field.options.length > 0}
										fallback={
											<Input
												aria-label={`Filter ${field.name}`}
												type={field.kind === "date" ? "date" : field.kind === "number" ? "number" : "text"}
												value={selectedFilter(field.id)}
												onChange={(event) => applyFilter(field, event.currentTarget.value)}
											/>
										}
									>
										<Select
											aria-label={`Filter ${field.name}`}
											value={selectedFilter(field.id)}
											onChange={(event) => applyFilter(field, event.currentTarget.value)}
										>
											<option value="">Any</option>
											<For each={field.options}>{(option) => <option value={option.id}>{option.name}</option>}</For>
										</Select>
									</Show>
								</label>
							)}
						</For>
						<label>
							Sort
							<Select
								aria-label="Sort tasks"
								value={query().sort?.fieldId ?? ""}
								onChange={(event) => {
									const fieldId = event.currentTarget.value;
									setQuery((previous) => ({
										...previous,
										savedFilterId: null,
										sort: fieldId ? { fieldId, descending: previous.sort?.descending ?? false } : null,
									}));
								}}
							>
								<option value="">Provider default</option>
								<For each={current().fields.filter((field) => field.sortable)}>
									{(field) => <option value={field.id}>{field.name}</option>}
								</For>
							</Select>
						</label>
						<button
							type="button"
							disabled={!query().sort}
							aria-label="Reverse task sort"
							onClick={() =>
								setQuery((previous) => ({
									...previous,
									savedFilterId: null,
									sort: previous.sort ? { ...previous.sort, descending: !previous.sort.descending } : null,
								}))
							}
						>
							{query().sort?.descending ? "Descending" : "Ascending"}
						</button>
						<label>
							Group
							<Select
								aria-label="Group tasks"
								value={query().groupBy ?? ""}
								onChange={(event) => {
									const groupBy = event.currentTarget.value || null;
									setQuery((previous) => ({ ...previous, savedFilterId: null, groupBy }));
								}}
							>
								<option value="">No grouping</option>
								<For each={current().fields.filter((field) => field.groupable)}>
									{(field) => <option value={field.id}>{field.name}</option>}
								</For>
							</Select>
						</label>
					</div>
				)}
			</Show>
			<div class={s.status} role="status" aria-live="polite">
				{model.state().loading ? "Loading tasks…" : `${model.state().items.length} items`}
				<Show when={model.state().stale}>
					<span class={s.stale}>Stale results</span>
				</Show>
				<Show when={model.state().error}>
					<span class={s.error}>{model.state().error}</span>
				</Show>
			</div>
			<Show when={!provider()}>
				<p class={s.empty}>Install a work-item provider to show tasks.</p>
			</Show>
			<Show when={provider() && !model.state().loading && !model.state().items.length && !model.state().error}>
				<p class={s.empty}>No tasks match this query.</p>
			</Show>
			<TaskRows
				items={model.state().items}
				groups={model.state().groups}
				loading={model.state().loading}
				resetKey={query()}
				onOpen={props.onOpen}
			/>
			<Show when={model.state().nextCursor}>
				<button
					class={s.more}
					type="button"
					disabled={model.state().loading || props.visible === false}
					onClick={() => void model.loadMore()}
				>
					Load more
				</button>
			</Show>
		</section>
	);
}
