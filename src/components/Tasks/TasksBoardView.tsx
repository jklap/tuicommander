import { Select } from "../ui/Select";
import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import type { WorkItemProvider } from "../../plugins/workItemProvider";
import type { WorkItemBoardPage, WorkItemQuery, WorkItemRef } from "../../types/work-items.gen";
import { TasksBoard } from "./board/TasksBoard";
import s from "./TasksShell.module.css";

const allItems: WorkItemQuery = {
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 50,
};
const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** Loads one provider board and feeds it to the host board; moves go back through the provider. */
export function TasksBoardView(props: {
	provider: WorkItemProvider;
	visible?: boolean;
	onOpen: (reference: WorkItemRef) => void;
}) {
	const [boardId, setBoardId] = createSignal("");
	const [page, setPage] = createSignal<WorkItemBoardPage>();
	const [error, setError] = createSignal("");
	let generation = 0;
	let controller: AbortController | undefined;
	const selected = () => props.provider.boards.find((board) => board.id === boardId()) ?? props.provider.boards[0];
	async function load() {
		const board = selected();
		controller?.abort();
		const attempt = ++generation;
		setError("");
		if (!board) {
			setPage(undefined);
			return;
		}
		controller = new AbortController();
		try {
			const next = await props.provider.board(board.id, allItems, controller.signal);
			if (attempt === generation) setPage(next);
		} catch (cause) {
			if (attempt === generation && !controller.signal.aborted) setError(message(cause));
		}
	}
	createEffect(() => {
		if (props.visible === false) return;
		selected();
		void load();
	});
	onCleanup(() => controller?.abort());
	return (
		<section class={s.view} aria-label="Tasks board">
			<Show when={props.provider.boards.length > 1}>
				<label class={s.boardPicker}>
					Board
					<Select value={selected()?.id} onChange={(event) => setBoardId(event.currentTarget.value)}>
						<For each={props.provider.boards}>{(board) => <option value={board.id}>{board.name}</option>}</For>
					</Select>
				</label>
			</Show>
			<Show when={error()}>
				<p class={s.error} role="alert">
					{error()}
				</p>
			</Show>
			<Show when={!selected()}>
				<p class={s.empty}>This provider has no boards.</p>
			</Show>
			<Show when={page()}>
				{(snapshot) => (
					<TasksBoard
						scopeKey={JSON.stringify([props.provider.providerId, props.provider.accountId])}
						board={snapshot().board}
						lanes={snapshot().lanes}
						onMove={(reference, action) => props.provider.act(reference, action)}
						onUpdated={() => void load()}
						onOpen={props.onOpen}
					/>
				)}
			</Show>
		</section>
	);
}
