import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
import { workItemProviderRegistry } from "../../plugins/workItemProviderRegistry";
import type { WorkItemRef } from "../../types/work-items.gen";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import { TasksCreate } from "./create/TasksCreate";
import { TasksDetail } from "./detail/TasksDetail";
import { TasksList } from "./list/TasksList";
import { TasksRoadmap } from "./roadmap/TasksRoadmap";
import { TasksBoardView } from "./TasksBoardView";
import s from "./TasksShell.module.css";

type View = "list" | "board" | "roadmap";
const VIEWS: readonly { id: View; label: string }[] = [
	{ id: "list", label: "List" },
	{ id: "board", label: "Board" },
	{ id: "roadmap", label: "Roadmap" },
];
const sourceKey = (providerId: string, accountId: string) => JSON.stringify([providerId, accountId]);

/** Core Tasks tab: mounts the host-owned work-item views over the registered providers. */
export function TasksShell(props: { visible?: () => boolean }) {
	const [source, setSource] = createSignal("");
	const [view, setView] = createSignal<View>("list");
	const [creating, setCreating] = createSignal(false);
	const [reference, setReference] = createSignal<WorkItemRef | null>(null);
	const providers = () => workItemProviderRegistry.list();
	const provider = createMemo(
		() => providers().find((entry) => sourceKey(entry.providerId, entry.accountId) === source()) ?? providers()[0],
	);
	function chooseSource(key: string) {
		setSource(key);
		setReference(null);
		setCreating(false);
	}
	function open(next: WorkItemRef) {
		setCreating(false);
		setReference(next);
	}
	return (
		<div class={s.shell} aria-label="Tasks shell">
			<Show when={provider()} fallback={<p class={s.empty}>Install a work-item provider plugin to show tasks.</p>}>
				{(current) => (
					<>
						<header class={s.header}>
							<Show when={providers().length > 1}>
								<label>
									Source
									<Select
										aria-label="Tasks source"
										value={sourceKey(current().providerId, current().accountId)}
										onChange={(event) => chooseSource(event.currentTarget.value)}
									>
										<For each={providers()}>
											{(entry) => <option value={sourceKey(entry.providerId, entry.accountId)}>{entry.name}</option>}
										</For>
									</Select>
								</label>
							</Show>
							<SegmentedControl
								options={VIEWS}
								value={creating() ? undefined : view()}
								aria-label="Tasks view"
								onChange={(id) => {
									setCreating(false);
									setView(id);
								}}
							/>
							<button type="button" class={s.create} onClick={() => setCreating(true)}>
								New task
							</button>
						</header>
						<div class={s.body} data-detail={reference() ? "open" : "closed"}>
							<div class={s.main}>
								<Switch>
									<Match when={creating()}>
										<TasksCreate
											provider={current()}
											onCreated={(detail) => open(detail.item.reference)}
											onCancel={() => setCreating(false)}
										/>
									</Match>
									<Match when={view() === "list"}>
										<TasksList providers={[current()]} onOpen={open} visible={props.visible?.()} />
									</Match>
									<Match when={view() === "board"}>
										<TasksBoardView provider={current()} onOpen={open} visible={props.visible?.()} />
									</Match>
									<Match when={view() === "roadmap"}>
										<TasksRoadmap provider={current()} onOpen={open} visible={props.visible?.()} />
									</Match>
								</Switch>
							</div>
							<Show when={reference()}>
								<aside class={s.side}>
									<button type="button" class={s.back} onClick={() => setReference(null)}>
										Back to list
									</button>
									<TasksDetail
										provider={current()}
										reference={reference()}
										visible={props.visible?.()}
										onClose={() => setReference(null)}
									/>
								</aside>
							</Show>
						</div>
					</>
				)}
			</Show>
		</div>
	);
}
