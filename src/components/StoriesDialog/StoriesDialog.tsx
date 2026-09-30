import { type Component, createEffect, createSignal, For, on, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import { HttpRpcError, isTauri } from "../../transport";
import d from "../shared/dialog.module.css";
import s from "./StoriesDialog.module.css";

interface Plan {
	id: string;
	project: string;
	title: string;
	source: string;
}
interface PlanSource {
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
	abandoned: boolean;
}
type Reply =
	| { type: "plan"; value: Plan }
	| { type: "plans"; value: Plan[] }
	| { type: "plan_sources"; value: PlanSource[] }
	| { type: "plan_state"; value: "draft" | "active" | "done" }
	| {
			type: "plan_view";
			value: { stories: Story[]; state: "draft" | "active" | "done"; wontFixCount: number; allCancelled: boolean };
	  }
	| { type: "story"; value: Story }
	| { type: "stories"; value: Story[] };

const missingBackendMessage = () =>
	t(
		"stories.error.missingBackend",
		"Restart TUICommander to load Plans and Stories. The running app needs a newer backend.",
	);

class MissingStoriesBackendError extends Error {
	constructor() {
		super(missingBackendMessage());
	}
}

const statusLabel = (status: Status): string => {
	switch (status) {
		case "backlog":
			return t("stories.status.backlog", "Backlog");
		case "ready":
			return t("stories.status.ready", "Ready");
		case "in_progress":
			return t("stories.status.inProgress", "In progress");
		case "review":
			return t("stories.status.review", "Review");
		case "done":
			return t("stories.status.done", "Done");
		case "blocked":
			return t("stories.status.blocked", "Blocked");
		case "wontfix":
			return t("stories.status.wontFix", "Won't fix");
	}
};

const planStateLabel = (state: string): string => {
	switch (state) {
		case "draft":
			return t("stories.planState.draft", "Draft");
		case "active":
			return t("stories.planState.active", "Active");
		case "done":
			return t("stories.planState.done", "Done");
		default:
			return state;
	}
};

export interface StoriesDialogProps {
	project: string;
	onClose: () => void;
}

export const StoriesDialog: Component<StoriesDialogProps> = (props) => {
	registerModal(props.onClose);
	const [plans, setPlans] = createSignal<Plan[]>([]);
	const [planSources, setPlanSources] = createSignal<PlanSource[]>([]);
	const [stories, setStories] = createSignal<Story[]>([]);
	const [planId, setPlanId] = createSignal<string | null>(null);
	const [storyId, setStoryId] = createSignal<string | null>(null);
	const [planState, setPlanState] = createSignal<string>("draft");
	const [wontFixCount, setWontFixCount] = createSignal(0);
	const [allCancelled, setAllCancelled] = createSignal(false);
	const [loading, setLoading] = createSignal(true);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [newPlan, setNewPlan] = createSignal(false);
	const [sourcesLoading, setSourcesLoading] = createSignal(false);
	const [newStory, setNewStory] = createSignal(false);
	const [planTitle, setPlanTitle] = createSignal("");
	const [planSource, setPlanSource] = createSignal("");
	const [storyTitle, setStoryTitle] = createSignal("");
	const [criteriaText, setCriteriaText] = createSignal("");
	const [scopeText, setScopeText] = createSignal("");
	const [priority, setPriority] = createSignal(2);
	const [dependencyId, setDependencyId] = createSignal("");
	let closeButton: HTMLButtonElement | undefined;
	let request = 0;
	let capabilitiesReady = false;
	// A dependency choice belongs to the story it was made for.
	createEffect(on(storyId, () => setDependencyId("")));

	const selectedPlan = () => plans().find((plan) => plan.id === planId());
	const selectedStory = () => stories().find((story) => story.id === storyId());
	const fail = (cause: unknown): void => {
		const message = String(cause);
		appLogger.warn("store", "Stories: action failed", { error: message });
		setError(cause instanceof Error ? cause.message : message);
	};
	const ensureCapabilities = async (): Promise<void> => {
		if (capabilitiesReady) return;
		try {
			if ((await invoke<unknown>("story_capabilities")) !== true) throw new MissingStoriesBackendError();
			capabilitiesReady = true;
		} catch (cause) {
			if (
				cause instanceof MissingStoriesBackendError ||
				isTauri() ||
				(cause instanceof HttpRpcError && cause.status === 404)
			) {
				throw new MissingStoriesBackendError();
			}
			throw cause;
		}
	};
	const call = async (action: Record<string, unknown>): Promise<Reply> => {
		const reply = await invoke<unknown>("story_action_command", { project: props.project, action });
		if (typeof reply === "string" && /^\s*<!doctype html|^\s*<html/i.test(reply)) {
			throw new Error(missingBackendMessage());
		}
		return reply as Reply;
	};

	async function refresh(preferredPlan = planId(), preferredStory = storyId()): Promise<void> {
		const current = ++request;
		setLoading(true);
		setError("");
		try {
			await ensureCapabilities();
			const list = await call({ action: "list_plans" });
			if (list.type !== "plans") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
			let available: PlanSource[] | undefined;
			if (list.value.length === 0) {
				const sources = await call({ action: "list_plan_sources" });
				if (sources.type !== "plan_sources") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
				available = sources.value;
			}
			const nextPlan = list.value.find((plan) => plan.id === preferredPlan)?.id ?? list.value[0]?.id ?? null;
			let nextStories: Story[] = [];
			let nextState = "draft";
			let nextWontFixCount = 0;
			let nextAllCancelled = false;
			if (nextPlan) {
				const view = await call({ action: "plan_view", plan_id: nextPlan });
				if (view.type !== "plan_view") throw new Error(t("stories.error.invalidStory", "Invalid story response"));
				nextStories = view.value.stories;
				nextState = view.value.state;
				nextWontFixCount = view.value.wontFixCount;
				nextAllCancelled = view.value.allCancelled;
			}
			if (current !== request) return;
			if (available) {
				setPlanSources(available);
				if (available.length > 0) setNewPlan(true);
			}
			setPlans(list.value);
			setPlanId(nextPlan);
			setStories(nextStories);
			setPlanState(nextState);
			setWontFixCount(nextWontFixCount);
			setAllCancelled(nextAllCancelled);
			setStoryId(nextStories.find((story) => story.id === preferredStory)?.id ?? nextStories[0]?.id ?? null);
		} catch (cause) {
			if (current === request) fail(cause);
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
			fail(cause);
			return undefined;
		} finally {
			setBusy(false);
		}
	}

	async function createPlan(event: Event): Promise<void> {
		event.preventDefault();
		const source = planSource().trim();
		const result = await addPlanSource(source, /^https?:\/\//i.test(source) ? planTitle().trim() : undefined);
		if (result) {
			setNewPlan(false);
			setPlanTitle("");
			setPlanSource("");
		}
	}
	async function loadPlanSources(): Promise<void> {
		setSourcesLoading(true);
		setError("");
		try {
			const reply = await call({ action: "list_plan_sources" });
			if (reply.type !== "plan_sources") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
			setPlanSources(reply.value);
		} catch (cause) {
			fail(cause);
		} finally {
			setSourcesLoading(false);
		}
	}
	async function addPlanSource(source: string, linkTitle?: string): Promise<boolean> {
		setBusy(true);
		setError("");
		try {
			const reply = await call(
				linkTitle === undefined
					? { action: "add_plan_source", source }
					: { action: "create_plan", title: linkTitle, source },
			);
			if (reply.type !== "plan") throw new Error(t("stories.error.invalidPlan", "Invalid plan response"));
			await refresh(reply.value.id, null);
			setPlanSources((sources) => sources.filter((candidate) => candidate.source !== source));
			setNewPlan(false);
			return true;
		} catch (cause) {
			fail(cause);
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
			setError(t("stories.error.criteriaRequired", "Add at least one acceptance criterion."));
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
			if (reply.type !== "story") throw new Error(t("stories.error.invalidStory", "Invalid story response"));
			await refresh(currentPlan, reply.value.id);
			setNewStory(false);
			setStoryTitle("");
			setCriteriaText("");
			setScopeText("");
		} catch (cause) {
			fail(cause);
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

	function removeDependency(dependencyId: string): void {
		const story = selectedStory();
		if (story?.status !== "backlog") return;
		void mutate({
			action: "remove_dependency",
			story_id: story.id,
			dependency_id: dependencyId,
			expected_revision: story.revision,
		});
	}

	onMount(() => {
		closeButton?.focus();
		void refresh();
	});

	return (
		<div class={d.overlay} onClick={props.onClose}>
			<div
				class={s.dialog}
				role="dialog"
				aria-modal="true"
				aria-label={t("stories.title", "Plans and Stories")}
				onClick={(event) => event.stopPropagation()}
			>
				<header class={s.header}>
					<div>
						<h2>{t("stories.title", "Plans and Stories")}</h2>
						<span class={s.project}>{props.project}</span>
					</div>
					<button
						ref={closeButton}
						type="button"
						class={s.iconButton}
						aria-label={t("stories.close", "Close Plans and Stories")}
						onClick={props.onClose}
					>
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
							{t("stories.retry", "Retry")}
						</button>
					</div>
				</Show>
				<Show when={loading()}>
					<div class={s.message} role="status">
						{t("stories.loading", "Loading plans and stories…")}
					</div>
				</Show>
				<div class={s.content}>
					<aside class={s.planColumn} aria-label={t("stories.plans", "Plans")}>
						<div class={s.columnHeader}>
							<h3>{t("stories.plans", "Plans")}</h3>
							<button
								type="button"
								onClick={() => {
									if (!newPlan()) void loadPlanSources();
									setNewPlan(!newPlan());
								}}
							>
								{t("stories.newPlan", "New plan")}
							</button>
						</div>
						<Show when={selectedPlan() && wontFixCount() > 0}>
							<p class={s.planNotice}>
								{allCancelled()
									? t("stories.allCancelled", "All cancelled")
									: t("stories.wontFixCount", "{count} won't fix", { count: String(wontFixCount()) })}
							</p>
						</Show>
						<Show when={newPlan()}>
							<div class={s.form}>
								<div class={s.sourceHeader}>
									<span>{t("stories.existingPlans", "Plans in this project")}</span>
									<button type="button" disabled={sourcesLoading()} onClick={() => void loadPlanSources()}>
										{t("stories.refreshPlans", "Refresh")}
									</button>
								</div>
								<Show when={sourcesLoading()}>
									<span>{t("stories.loading", "Loading plans and stories…")}</span>
								</Show>
								<For each={planSources().filter((source) => !plans().some((plan) => plan.source === source.source))}>
									{(candidate) => (
										<button type="button" disabled={busy()} onClick={() => void addPlanSource(candidate.source)}>
											{candidate.title}
											<small>{candidate.source}</small>
										</button>
									)}
								</For>
								<details>
									<summary>{t("stories.addFromPath", "Add from path or link")}</summary>
									<form class={s.manualForm} onSubmit={(event) => void createPlan(event)}>
										<label>
											{t("stories.planSource", "Plan document or link")}
											<input
												required
												value={planSource()}
												onInput={(event) => setPlanSource(event.currentTarget.value)}
											/>
										</label>
										<Show when={/^https?:\/\//i.test(planSource())}>
											<label>
												{t("stories.titleLabel", "Title")}
												<input
													required
													maxlength="200"
													value={planTitle()}
													onInput={(event) => setPlanTitle(event.currentTarget.value)}
												/>
											</label>
										</Show>
										<button type="submit" disabled={busy()}>
											{t("stories.createPlan", "Create plan")}
										</button>
									</form>
								</details>
							</div>
						</Show>
						<Show when={!loading() && plans().length === 0}>
							<p class={s.empty}>{t("stories.emptyPlans", "Create a plan to begin.")}</p>
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
					<section class={s.storyColumn} aria-label={t("stories.stories", "Stories")}>
						<div class={s.columnHeader}>
							<h3>{t("stories.stories", "Stories")}</h3>
							<button type="button" disabled={!planId()} onClick={() => setNewStory(!newStory())}>
								{t("stories.newStory", "New story")}
							</button>
						</div>
						<Show when={newStory() && planId()}>
							<form class={s.form} onSubmit={(event) => void createStory(event)}>
								<label>
									{t("stories.titleLabel", "Title")}
									<input
										required
										maxlength="200"
										value={storyTitle()}
										onInput={(event) => setStoryTitle(event.currentTarget.value)}
									/>
								</label>
								<label>
									{t("stories.criteriaInput", "Acceptance criteria, one per line")}
									<textarea
										required
										value={criteriaText()}
										onInput={(event) => setCriteriaText(event.currentTarget.value)}
									/>
								</label>
								<label>
									{t("stories.scopeInput", "File scope, one relative path per line")}
									<textarea value={scopeText()} onInput={(event) => setScopeText(event.currentTarget.value)} />
								</label>
								<label>
									{t("stories.priority", "Priority")}
									<select value={priority()} onChange={(event) => setPriority(Number(event.currentTarget.value))}>
										<option value="1">P1</option>
										<option value="2">P2</option>
										<option value="3">P3</option>
									</select>
								</label>
								<button type="submit" disabled={busy()}>
									{t("stories.createStory", "Create story")}
								</button>
							</form>
						</Show>
						<Show when={!loading() && planId() && stories().length === 0}>
							<p class={s.empty}>{t("stories.emptyStories", "This plan has no stories.")}</p>
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
										<small>{statusLabel(story.status)}</small>
									</button>
								)}
							</For>
						</nav>
					</section>
					<section class={s.detail} aria-label={t("stories.details", "Story details")}>
						<Show
							when={selectedStory()}
							fallback={
								<Show
									when={selectedPlan()}
									fallback={<p class={s.empty}>{t("stories.selectPlan", "Select or create a plan.")}</p>}
								>
									{(plan) => (
										<div class={s.planDetail}>
											<h3>{plan().title}</h3>
											<p>
												{t("stories.source", "Source")}: {plan().source}
											</p>
											<p>
												{t("stories.state", "State")}: {planStateLabel(planState())}
											</p>
										</div>
									)}
								</Show>
							}
						>
							{(story) => (
								<>
									<div class={s.detailHeader}>
										<div>
											<span class={s.eyebrow}>
												{t("stories.story", "Story")} · P{story().priority}
											</span>
											<h3>{story().title}</h3>
										</div>
										<span class={s.status} data-status={story().status}>
											{statusLabel(story().status)}
										</span>
									</div>
									<section>
										<h4>{t("stories.criteria", "Acceptance criteria")}</h4>
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
										<h4>{t("stories.dependencies", "Dependencies")}</h4>
										<Show
											when={story().dependencies.length}
											fallback={<p class={s.muted}>{t("stories.none", "None")}</p>}
										>
											<ul>
												<For each={story().dependencies}>
													{(id) => {
														const dependency = stories().find((item) => item.id === id);
														return (
															<li class={s.dependencyRow}>
																<span>
																	{dependency?.title ?? id} ·{" "}
																	{dependency ? statusLabel(dependency.status) : t("stories.unknown", "Unknown")}
																	{dependency?.abandoned ? ` · ${t("stories.abandoned", "abandoned")}` : ""}
																</span>
																<Show when={story().status === "backlog" && dependency?.status === "wontfix"}>
																	<button
																		type="button"
																		disabled={busy()}
																		aria-label={t("stories.removeNamedDependency", "Remove {title}", {
																			title: dependency?.title ?? id,
																		})}
																		onClick={() => removeDependency(id)}
																	>
																		{t("stories.remove", "Remove")}
																	</button>
																</Show>
															</li>
														);
													}}
												</For>
											</ul>
										</Show>
										<Show when={story().status === "ready" || story().status === "backlog"}>
											<div class={s.inline}>
												<select
													aria-label={t("stories.addDependency", "Add dependency")}
													value={dependencyId()}
													onChange={(event) => setDependencyId(event.currentTarget.value)}
												>
													<option value="">{t("stories.selectStory", "Select story")}</option>
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
													{t("stories.addDependency", "Add dependency")}
												</button>
											</div>
										</Show>
									</section>
									<section>
										<h4>{t("stories.fileScope", "File scope")}</h4>
										<Show
											when={story().fileScope.length}
											fallback={<p class={s.muted}>{t("stories.unspecified", "Unspecified")}</p>}
										>
											<ul>
												<For each={story().fileScope}>{(path) => <li class={s.path}>{path}</li>}</For>
											</ul>
										</Show>
									</section>
									<div class={s.actions}>
										<Show when={story().status === "ready"}>
											<button type="button" disabled={busy()} onClick={() => transition("start_manual")}>
												{t("stories.startWork", "Start work")}
											</button>
										</Show>
										<Show when={story().status === "in_progress"}>
											<button type="button" disabled={busy()} onClick={() => transition("submit_review")}>
												{t("stories.submitReview", "Submit for review")}
											</button>
										</Show>
										<Show when={story().status === "review"}>
											<button type="button" disabled={busy()} onClick={() => transition("approve")}>
												{t("stories.approve", "Approve")}
											</button>
											<button type="button" disabled={busy()} onClick={() => transition("reject_review")}>
												{t("stories.requestChanges", "Request changes")}
											</button>
										</Show>
										<Show when={["ready", "in_progress", "review"].includes(story().status)}>
											<button type="button" disabled={busy()} onClick={() => transition("block")}>
												{t("stories.block", "Block")}
											</button>
										</Show>
										<Show when={story().status === "blocked"}>
											<button type="button" disabled={busy()} onClick={() => transition("unblock")}>
												{t("stories.unblock", "Unblock")}
											</button>
										</Show>
										<Show when={story().status !== "done" && story().status !== "wontfix"}>
											<button type="button" class={s.danger} disabled={busy()} onClick={() => transition("wont_fix")}>
												{t("stories.status.wontFix", "Won't fix")}
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
