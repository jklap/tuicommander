import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import type { WorkItemProvider } from "../../plugins/workItemProvider";
import type { WorkItemRef } from "../../types/work-items.gen";
import { Select } from "../ui/Select";
import { createBoardPages } from "./board/createBoardPages";
import { TasksBoard } from "./board/TasksBoard";
import { StartWorkMenu } from "./StartWorkMenu";
import s from "./TasksShell.module.css";

/** Loads one provider board and feeds it to the host board; moves go back through the provider. */
export function TasksBoardView(props: {
	provider: WorkItemProvider;
	visible?: boolean;
	/** Repository that Start work creates its worktree in. */
	repoPath?: string;
	onOpen: (reference: WorkItemRef) => void;
}) {
	const [boardId, setBoardId] = createSignal("");
	const pages = createBoardPages();
	const selected = () => props.provider.boards.find((board) => board.id === boardId()) ?? props.provider.boards[0];
	createEffect(() => {
		const provider = props.provider;
		const id = selected()?.id ?? "";
		if (props.visible === false) {
			pages.cancel();
			return;
		}
		pages.select(provider, id);
	});
	onCleanup(pages.cancel);
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
			<Show when={pages.error()}>
				<p class={s.error} role="alert">
					{pages.error()}
				</p>
			</Show>
			<Show when={!selected()}>
				<p class={s.empty}>This provider has no boards.</p>
			</Show>
			<Show when={pages.page()}>
				{(snapshot) => (
					<TasksBoard
						scopeKey={JSON.stringify([props.provider.providerId, props.provider.accountId])}
						board={snapshot().board}
						lanes={snapshot().lanes}
						onMove={(reference, action) => props.provider.act(reference, action)}
						onUpdated={() => void pages.refresh()}
						hasMore={!!snapshot().nextCursor}
						loadingMore={pages.loading()}
						onMore={() => void pages.more()}
						onOpen={props.onOpen}
						cardActions={(card) => (
							<StartWorkMenu provider={props.provider} item={card.item} repoPath={props.repoPath} />
						)}
					/>
				)}
			</Show>
		</section>
	);
}
