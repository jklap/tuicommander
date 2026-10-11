import { createVirtualizer } from "@tanstack/solid-virtual";
import { type Component, For, Show } from "solid-js";
import type { WorkItemBoardColumn } from "../../../types/work-items.gen";
import s from "./board.module.css";
import { type BoardCard, cardKey } from "./types";

interface BoardLaneProps {
	column: WorkItemBoardColumn;
	columns: WorkItemBoardColumn[];
	cards: readonly BoardCard[];
	target: boolean;
	pending(key: string): boolean;
	canMove(card: BoardCard, column: string): boolean;
	onMove(card: BoardCard, column: string): void;
	onPointerDown(event: PointerEvent, card: BoardCard): void;
	onOpen?(card: BoardCard): void;
}
export const BoardLane: Component<BoardLaneProps> = (props) => {
	let scroll: HTMLDivElement | undefined;
	const virtualizer = createVirtualizer({
		get count() {
			return props.cards.length;
		},
		getScrollElement: () => scroll ?? null,
		estimateSize: () => 132,
		overscan: 3,
		getItemKey: (index) => {
			const card = props.cards[index];
			return card ? cardKey(card.item.reference) : index;
		},
		initialRect: { width: 280, height: 600 },
	});
	return (
		<section
			role="region"
			aria-label={props.column.name}
			data-board-column={props.column.id}
			class={s.lane}
			classList={{ [s.dropTarget]: props.target }}
		>
			<header class={s.laneHeader}>
				<h3>{props.column.name}</h3>
				<span>{props.cards.length}</span>
			</header>
			<div class={s.laneScroll} ref={scroll}>
				<Show when={props.cards.length === 0}>
					<p class={s.empty}>No items</p>
				</Show>
				<div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
					<For each={virtualizer.getVirtualItems()}>
						{(row) => {
							const card = () => props.cards[row.index];
							return (
								<Show when={card()}>
									{(entry) => (
										<article
											class={s.card}
											data-board-card={entry().item.key}
											aria-label={entry().item.key}
											aria-busy={props.pending(cardKey(entry().item.reference))}
											style={{ position: "absolute", top: `${row.start}px`, height: `${row.size}px`, width: "100%" }}
											onPointerDown={(event) => props.onPointerDown(event, entry())}
										>
											<button type="button" class={s.key} onClick={() => props.onOpen?.(entry())}>
												{entry().item.key}
											</button>
											<span class={s.title}>{entry().item.title}</span>
											<Show
												when={props.pending(cardKey(entry().item.reference))}
												fallback={
													<select
														aria-label={`Move ${entry().item.key}`}
														value={props.column.id}
														onChange={(event) => {
															props.onMove(entry(), event.currentTarget.value);
															event.currentTarget.value = props.column.id;
														}}
													>
														<For each={props.columns}>
															{(column) => (
																<option
																	value={column.id}
																	disabled={column.id !== props.column.id && !props.canMove(entry(), column.id)}
																>
																	{column.name}
																</option>
															)}
														</For>
													</select>
												}
											>
												<span class={s.pending}>Moving {entry().item.key}…</span>
											</Show>
										</article>
									)}
								</Show>
							);
						}}
					</For>
				</div>
			</div>
		</section>
	);
};
