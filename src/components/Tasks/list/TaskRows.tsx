import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, For, Show } from "solid-js";
import type { WorkItem, WorkItemGroup, WorkItemRef } from "../../../types/work-items.gen";
import s from "./tasksList.module.css";

interface TaskRowsProps {
	items: WorkItem[];
	groups: WorkItemGroup[];
	loading: boolean;
	resetKey: unknown;
	onOpen: (reference: WorkItemRef) => void;
}
type DisplayRow = { kind: "group"; group: WorkItemGroup } | { kind: "item"; item: WorkItem; groupId: string };

/** Flatten only the provider's presentation groups; never infer group membership. */
export function TaskRows(props: TaskRowsProps) {
	let scroll: HTMLDivElement | undefined;
	const rows = createMemo<DisplayRow[]>(() =>
		props.groups.length
			? props.groups.flatMap((group): DisplayRow[] => [
					{ kind: "group", group },
					...group.items.map((item): DisplayRow => ({ kind: "item", item, groupId: group.id })),
				])
			: props.items.map((item) => ({ kind: "item", item, groupId: "" })),
	);
	createEffect(() => {
		props.resetKey;
		if (scroll) scroll.scrollTop = 0;
	});
	const virtualizer = createVirtualizer({
		get count() {
			return rows().length;
		},
		getScrollElement: () => scroll ?? null,
		estimateSize: () => 48,
		overscan: 5,
		getItemKey: (index) => {
			const row = rows()[index];
			return row?.kind === "group" ? `group:${row.group.id}` : JSON.stringify([row?.groupId, row?.item.reference]);
		},
	});
	return (
		<div class={s.viewport} ref={scroll} role="list" aria-label="Task results" aria-busy={props.loading}>
			<div class={s.canvas} style={{ height: `${virtualizer.getTotalSize()}px` }}>
				<For each={virtualizer.getVirtualItems()}>
					{(row) => {
						const entry = () => rows()[row.index];
						return (
							<div class={s.row} style={{ top: `${row.start}px`, height: `${row.size}px` }}>
								<Show when={entry()} keyed>
									{(current) => {
										if (current.kind === "group") return <h3 class={s.group}>{current.group.name}</h3>;
										return (
											<div role="listitem" aria-posinset={row.index + 1} aria-setsize={rows().length}>
												<button type="button" class={s.item} onClick={() => props.onOpen(current.item.reference)}>
													<span class={s.key}>{current.item.key}</span>
													<span class={s.title}>{current.item.title}</span>
													<span class={s.state} data-tone={current.item.state.tone}>
														{current.item.state.name}
													</span>
												</button>
											</div>
										);
									}}
								</Show>
							</div>
						);
					}}
				</For>
			</div>
		</div>
	);
}
