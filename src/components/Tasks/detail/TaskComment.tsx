import { For, Show } from "solid-js";
import type { WorkItemComment, WorkItemCommentEntry } from "../../../types/work-items.gen";
import { TaskMarkdown } from "./TaskMarkdown";
import styles from "./TasksDetail.module.css";

function Comment(props: { comment: WorkItemComment }) {
	return (
		<article class={styles.comment}>
			<header class={styles.commentHeader}>
				<strong>{props.comment.author.name}</strong>
				<time dateTime={props.comment.createdAt}>{new Date(props.comment.createdAt).toLocaleString()}</time>
			</header>
			<TaskMarkdown content={props.comment.body} />
		</article>
	);
}

export function TaskComment(props: { entry: WorkItemCommentEntry }) {
	const normal = () => (props.entry.kind === "normal" ? props.entry.value : undefined);
	const thread = () => (props.entry.kind === "review_thread" ? props.entry.value : undefined);
	const summary = () => (props.entry.kind === "review_summary" ? props.entry.value : undefined);
	return (
		<>
			<Show when={normal()}>{(comment) => <Comment comment={comment()} />}</Show>
			<Show when={thread()}>
				{(value) => (
					<section class={styles.thread} aria-label="Review thread">
						<header>
							{value().path}
							{value().line !== null ? `:${value().line}` : ""} · {value().resolved ? "Resolved" : "Open thread"}
						</header>
						<For each={value().comments.map((comment) => comment.id)}>
							{(id) => {
								const comment = () => value().comments.find((entry) => entry.id === id);
								return <Show when={comment()}>{(entry) => <Comment comment={entry()} />}</Show>;
							}}
						</For>
					</section>
				)}
			</Show>
			<Show when={summary()}>
				{(value) => (
					<section class={styles.thread} aria-label="Review summary">
						<header>Review · {value().state}</header>
						<Comment comment={value().comment} />
					</section>
				)}
			</Show>
		</>
	);
}
