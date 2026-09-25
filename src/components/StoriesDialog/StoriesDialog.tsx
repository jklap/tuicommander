import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { registerModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";
import s from "./StoriesDialog.module.css";

interface Plan {
	id: string;
	project: string;
	title: string;
	source: string;
}
type Status = "backlog" | "ready" | "in_progress" | "review" | "done" | "blocked" | "wontfix";
interface Story {
	id: string;
	planId: string;
	title: string;
	criteria: string[];
	checked: boolean[];
	dependencies: string[];
	priority: number;
	origin: unknown;
	fileScope: string[];
	status: Status;
	revision: number;
	claimSession: string | null;
}
type Reply =
	| { type: "plan"; value: Plan }
	| { type: "plans"; value: Plan[] }
	| { type: "plan_state"; value: "draft" | "active" | "done" }
	| { type: "story"; value: Story }
	| { type: "stories"; value: Story[] };

export interface StoriesDialogProps {
	project: string;
	onClose: () => void;
}

export const StoriesDialog: Component<StoriesDialogProps> = (props) => {
	registerModal(props.onClose);
	const [plans, setPlans] = createSignal<Plan[]>([]);
	const [stories, setStories] = createSignal<Story[]>([]);
	const [planId, setPlanId] = createSignal<string | null>(null);
	const [storyId, setStoryId] = createSignal<string | null>(null);
	const [planState, setPlanState] = createSignal<string>("draft");
	const [loading, setLoading] = createSignal(true);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [newPlan, setNewPlan] = createSignal(false);
	const [newStory, setNewStory] = createSignal(false);
	const [planTitle, setPlanTitle] = createSignal("");
	const [planSource, setPlanSource] = createSignal("");
	const [storyTitle, setStoryTitle] = createSignal("");
	const [criteriaText, setCriteriaText] = createSignal("");
	const [scopeText, setScopeText] = createSignal("");
	const [priority, setPriority] = createSignal(2);
	const [dependencyId, setDependencyId] = createSignal("");
	let request = 0;

	const selectedPlan = () => plans().find((plan) => plan.id === planId());
	const selectedStory = () => stories().find((story) => story.id === storyId());
	const call = (action: Record<string, unknown>) =>
		invoke<Reply>("story_action_command", { project: props.project, action });

	async function refresh(preferredPlan = planId(), preferredStory = storyId()): Promise<void> {
		const current = ++request;
		setLoading(true);
		setError("");
		try {
			const list = await call({ action: "list_plans" });
			if (list.type !== "plans") throw new Error("Invalid plan response");
			const nextPlan = list.value.find((plan) => plan.id === preferredPlan)?.id ?? list.value[0]?.id ?? null;
			let nextStories: Story[] = [];
			let nextState = "draft";
			if (nextPlan) {
				const [rows, state] = await Promise.all([
					call({ action: "list_stories", plan_id: nextPlan }),
					call({ action: "plan_state", plan_id: nextPlan }),
				]);
				if (rows.type !== "stories" || state.type !== "plan_state") throw new Error("Invalid story response");
				nextStories = rows.value;
				nextState = state.value;
			}
			if (current !== request) return;
			setPlans(list.value);
			setPlanId(nextPlan);
			setStories(nextStories);
			setPlanState(nextState);
			setStoryId(nextStories.find((story) => story.id === preferredStory)?.id ?? nextStories[0]?.id ?? null);
		} catch (cause) {
			if (current === request) setError(String(cause));
		} finally {
			if (current === request) setLoading(false);
		}
	}

	async function mutate(
		action: Record<string, unknown>,
		preferredPlan = planId(),
		preferredStory = storyId(),
	): Promise<Reply | undefined> {
		setBusy(true);
		setError("");
		try {
			const result = await call(action);
			await refresh(preferredPlan, preferredStory);
			return result;
		} catch (cause) {
			setError(String(cause));
			return undefined;
		} finally {
			setBusy(false);
		}
	}

	async function createPlan(event: Event): Promise<void> {
		event.preventDefault();
		const result = await callMutationPlan();
		if (result) {
			setNewPlan(false);
			setPlanTitle("");
			setPlanSource("");
		}
	}
	async function callMutationPlan(): Promise<boolean> {
		setBusy(true);
		setError("");
		try {
			const reply = await call({ action: "create_plan", title: planTitle().trim(), source: planSource().trim() });
			if (reply.type !== "plan") throw new Error("Invalid plan response");
			await refresh(reply.value.id, null);
			return true;
		} catch (cause) {
			setError(String(cause));
			return false;
		} finally {
			setBusy(false);
		}
	}

	async function createStory(event: Event): Promise<void> {
		event.preventDefault();
		const currentPlan = planId();
		if (!currentPlan) return;
		const criteria = criteriaText()
			.split(/\r?\n/)
			.map((line) => line.trim())
			.filter(Boolean);
		const fileScope = scopeText()
			.split(/\r?\n/)
			.map((line) => line.trim())
			.filter(Boolean);
		if (criteria.length === 0) {
			setError("Add at least one acceptance criterion.");
			return;
		}
		setBusy(true);
		setError("");
		try {
			const reply = await call({
				action: "create_story",
				input: {
					planId: currentPlan,
					title: storyTitle().trim(),
					criteria,
					priority: priority(),
					origin: { type: "native" },
					fileScope,
				},
			});
			if (reply.type !== "story") throw new Error("Invalid story response");
			await refresh(currentPlan, reply.value.id);
			setNewStory(false);
			setStoryTitle("");
			setCriteriaText("");
			setScopeText("");
		} catch (cause) {
			setError(String(cause));
		} finally {
			setBusy(false);
		}
	}

	function transition(command: string | Record<string, number>): void {
		const story = selectedStory();
		if (!story) return;
		void mutate({ action: "transition", story_id: story.id, expected_revision: story.revision, command });
	}

	function addDependency(): void {
		const story = selectedStory();
		const dep = dependencyId();
		if (!story || !dep) return;
		void mutate({
			action: "add_dependency",
			story_id: story.id,
			dependency_id: dep,
			expected_revision: story.revision,
		});
		setDependencyId("");
	}

	onMount(() => void refresh());

	return (
		<div class={d.overlay} onClick={props.onClose}>
			<div
				class={s.dialog}
				role="dialog"
				aria-modal="true"
				aria-label="Plans and Stories"
				onClick={(event) => event.stopPropagation()}
			>
				<header class={s.header}>
					<div>
						<h2>Plans and Stories</h2>
						<span class={s.project}>{props.project}</span>
					</div>
					<button type="button" class={s.iconButton} aria-label="Close Plans and Stories" onClick={props.onClose}>
						<svg
							viewBox="0 0 16 16"
							width="15"
							height="15"
							fill="none"
							stroke="currentColor"
							stroke-width="1.5"
							aria-hidden="true"
						>
							<path d="M3 3l10 10M13 3 3 13" />
						</svg>
					</button>
				</header>
				<Show when={error()}>
					<div class={s.error} role="alert">
						{error()}{" "}
						<button type="button" onClick={() => void refresh()}>
							Retry
						</button>
					</div>
				</Show>
				<Show when={loading()}>
					<div class={s.message} role="status">
						Loading plans and stories…
					</div>
				</Show>
				<div class={s.content}>
					<aside class={s.planColumn} aria-label="Plans">
						<div class={s.columnHeader}>
							<h3>Plans</h3>
							<button type="button" onClick={() => setNewPlan(!newPlan())}>
								New plan
							</button>
						</div>
						<Show when={newPlan()}>
							<form class={s.form} onSubmit={(event) => void createPlan(event)}>
								<label>
									Title
									<input
										required
										maxlength="200"
										value={planTitle()}
										onInput={(event) => setPlanTitle(event.currentTarget.value)}
									/>
								</label>
								<label>
									Plan document or link
									<input required value={planSource()} onInput={(event) => setPlanSource(event.currentTarget.value)} />
								</label>
								<button type="submit" disabled={busy()}>
									Create plan
								</button>
							</form>
						</Show>
						<Show when={!loading() && plans().length === 0}>
							<p class={s.empty}>Create a plan to begin.</p>
						</Show>
						<nav class={s.items}>
							<For each={plans()}>
								{(plan) => (
									<button
										type="button"
										class={s.item}
										aria-current={planId() === plan.id ? "true" : undefined}
										onClick={() => void refresh(plan.id, null)}
									>
										{plan.title}
									</button>
								)}
							</For>
						</nav>
					</aside>
					<section class={s.storyColumn} aria-label="Stories">
						<div class={s.columnHeader}>
							<h3>Stories</h3>
							<button type="button" disabled={!planId()} onClick={() => setNewStory(!newStory())}>
								New story
							</button>
						</div>
						<Show when={newStory() && planId()}>
							<form class={s.form} onSubmit={(event) => void createStory(event)}>
								<label>
									Title
									<input
										required
										maxlength="200"
										value={storyTitle()}
										onInput={(event) => setStoryTitle(event.currentTarget.value)}
									/>
								</label>
								<label>
									Acceptance criteria, one per line
									<textarea
										required
										value={criteriaText()}
										onInput={(event) => setCriteriaText(event.currentTarget.value)}
									/>
								</label>
								<label>
									File scope, one relative path per line
									<textarea value={scopeText()} onInput={(event) => setScopeText(event.currentTarget.value)} />
								</label>
								<label>
									Priority
									<select value={priority()} onChange={(event) => setPriority(Number(event.currentTarget.value))}>
										<option value="1">P1</option>
										<option value="2">P2</option>
										<option value="3">P3</option>
									</select>
								</label>
								<button type="submit" disabled={busy()}>
									Create story
								</button>
							</form>
						</Show>
						<Show when={!loading() && planId() && stories().length === 0}>
							<p class={s.empty}>This plan has no stories.</p>
						</Show>
						<nav class={s.items}>
							<For each={stories()}>
								{(story) => (
									<button
										type="button"
										class={s.item}
										aria-current={storyId() === story.id ? "true" : undefined}
										onClick={() => setStoryId(story.id)}
									>
										<span>{story.title}</span>
										<small>{story.status.replaceAll("_", " ")}</small>
									</button>
								)}
							</For>
						</nav>
					</section>
					<section class={s.detail} aria-label="Story details">
						<Show
							when={selectedStory()}
							fallback={
								<Show when={selectedPlan()} fallback={<p class={s.empty}>Select or create a plan.</p>}>
									{(plan) => (
										<div class={s.planDetail}>
											<h3>{plan().title}</h3>
											<p>Source: {plan().source}</p>
											<p>State: {planState()}</p>
										</div>
									)}
								</Show>
							}
						>
							{(story) => (
								<>
									<div class={s.detailHeader}>
										<div>
											<span class={s.eyebrow}>Story · P{story().priority}</span>
											<h3>{story().title}</h3>
										</div>
										<span class={s.status} data-status={story().status}>
											{story().status.replaceAll("_", " ")}
										</span>
									</div>
									<section>
										<h4>Acceptance criteria</h4>
										<ul class={s.criteria}>
											<For each={story().criteria}>
												{(criterion, index) => (
													<li>
														<label>
															<input
																type="checkbox"
																checked={story().checked[index()]}
																disabled={story().status !== "in_progress" || busy()}
																onChange={() =>
																	transition({
																		[story().checked[index()] ? "uncheck_criterion" : "check_criterion"]: index(),
																	})
																}
															/>
															<span>{criterion}</span>
														</label>
													</li>
												)}
											</For>
										</ul>
									</section>
									<section>
										<h4>Dependencies</h4>
										<Show when={story().dependencies.length} fallback={<p class={s.muted}>None</p>}>
											<ul>
												<For each={story().dependencies}>
													{(id) => <li>{stories().find((item) => item.id === id)?.title ?? id}</li>}
												</For>
											</ul>
										</Show>
										<Show when={story().status === "ready" || story().status === "backlog"}>
											<div class={s.inline}>
												<select
													aria-label="Add dependency"
													value={dependencyId()}
													onChange={(event) => setDependencyId(event.currentTarget.value)}
												>
													<option value="">Select story</option>
													<For
														each={stories().filter(
															(candidate) =>
																candidate.id !== story().id && !story().dependencies.includes(candidate.id),
														)}
													>
														{(candidate) => <option value={candidate.id}>{candidate.title}</option>}
													</For>
												</select>
												<button type="button" disabled={!dependencyId() || busy()} onClick={addDependency}>
													Add dependency
												</button>
											</div>
										</Show>
									</section>
									<section>
										<h4>File scope</h4>
										<Show when={story().fileScope.length} fallback={<p class={s.muted}>Unspecified</p>}>
											<ul>
												<For each={story().fileScope}>{(path) => <li class={s.path}>{path}</li>}</For>
											</ul>
										</Show>
									</section>
									<div class={s.actions}>
										<Show when={story().status === "ready"}>
											<button type="button" disabled={busy()} onClick={() => transition("start_manual")}>
												Start work
											</button>
										</Show>
										<Show when={story().status === "in_progress"}>
											<button type="button" disabled={busy()} onClick={() => transition("submit_review")}>
												Submit for review
											</button>
										</Show>
										<Show when={story().status === "review"}>
											<button type="button" disabled={busy()} onClick={() => transition("approve")}>
												Approve
											</button>
											<button type="button" disabled={busy()} onClick={() => transition("reject_review")}>
												Request changes
											</button>
										</Show>
										<Show when={["ready", "in_progress", "review"].includes(story().status)}>
											<button type="button" disabled={busy()} onClick={() => transition("block")}>
												Block
											</button>
										</Show>
										<Show when={story().status === "blocked"}>
											<button type="button" disabled={busy()} onClick={() => transition("unblock")}>
												Unblock
											</button>
										</Show>
										<Show when={story().status !== "done" && story().status !== "wontfix"}>
											<button type="button" class={s.danger} disabled={busy()} onClick={() => transition("wont_fix")}>
												Won't fix
											</button>
										</Show>
									</div>
								</>
							)}
						</Show>
					</section>
				</div>
			</div>
		</div>
	);
};
