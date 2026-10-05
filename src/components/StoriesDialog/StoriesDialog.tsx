import { type Component, createEffect, createSignal, For, on, onMount, Show, untrack } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import { workflowRunSignals } from "../../stores/workflowRunSignals";
import { HttpRpcError, isTauri } from "../../transport";
import d from "../shared/dialog.module.css";
import { WorkflowDesigner } from "../WorkflowDesigner/WorkflowDesigner";
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

interface RunGraph {
	id: string;
	targetId: string;
	definition: {
		id: string;
		revision: number;
		graph: {
			nodes: { id: string; kind: { type: string } }[];
			edges: { from: string; to: string; outcome?: string | null }[];
		};
	};
	activations: { id: string; nodeId: string; state: string; edgeIndex: number | null }[];
	decisions: {
		activationId: string;
		edgeIndex: number;
		evidence: { actor: string; reason: string; references: string[] };
	}[];
	loops: { nodeId: string; repeats: number }[];
	pauses: { activationId: string; resumeTo: string; evidence: { reason: string }; resolution: string | null }[];
	completed: boolean;
}
interface RunSnapshot {
	id: string;
	planId: string;
	status: string;
	sequence: number;
	startedMs: number;
	rootTarget?: { type: string; id: string } | null;
	graphExecutions?: RunGraph[];
	stories: { storyId: string; accepted: boolean }[];
	attempts: {
		id: string;
		storyId: string;
		nodeId: string;
		state: string;
		outcome: string | null;
		inputAnswer?: string | null;
		report?: { inputRequest?: { question: string; options: string[] } | null } | null;
	}[];
}
interface RunEvent {
	sequence: number;
	atMs: number;
	kind: { type: string; [field: string]: unknown };
}
type RunReply =
	| { type: "runs"; value: RunSnapshot[] }
	| { type: "snapshot"; value: RunSnapshot }
	| { type: "events"; value: RunEvent[] }
	| { type: "receipt"; value: { snapshot: RunSnapshot; event?: RunEvent } };

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
	const [showRuns, setShowRuns] = createSignal(false);
	const [showDesigner, setShowDesigner] = createSignal(false);
	const [runs, setRuns] = createSignal<RunSnapshot[]>([]);
	const [selectedRunId, setSelectedRunId] = createSignal<string | null>(null);
	const [run, setRun] = createSignal<RunSnapshot | null>(null);
	const [runEvents, setRunEvents] = createSignal<RunEvent[]>([]);
	const [runLoading, setRunLoading] = createSignal(false);
	const [runError, setRunError] = createSignal("");
	const [inputAnswer, setInputAnswer] = createSignal("");
	const [startOpen, setStartOpen] = createSignal(false);
	const [publishedChoices, setPublishedChoices] = createSignal<
		{ id: string; name: string; latestPublishedRevision: number }[]
	>([]);
	const [workflowChoice, setWorkflowChoice] = createSignal("");
	const [recoveryExecution, setRecoveryExecution] = createSignal("");
	const [recoveryActivation, setRecoveryActivation] = createSignal("");
	const [resolution, setResolution] = createSignal("");
	let startRequestId = "";
	createEffect(on(storyId, () => setStartOpen(false)));
	createEffect(
		on(selectedRunId, () => {
			setRecoveryExecution("");
			setRecoveryActivation("");
			setResolution("");
		}),
	);
	let closeButton: HTMLButtonElement | undefined;
	let request = 0;
	let runRequest = 0;
	let capabilitiesReady = false;
	// A dependency choice belongs to the story it was made for.
	createEffect(on(storyId, () => setDependencyId("")));

	const selectedPlan = () => plans().find((plan) => plan.id === planId());
	const selectedStory = () => stories().find((story) => story.id === storyId());
	const pendingInput = () =>
		run()?.attempts.find(
			(attempt) => attempt.outcome === "needs_input" && !attempt.inputAnswer && attempt.report?.inputRequest,
		);

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

	const callRun = (action: Record<string, unknown>) =>
		invoke<RunReply>("workflow_run_action", { project: props.project, action });

	async function loadRun(runId: string): Promise<void> {
		const current = ++runRequest;
		const after = untrack(() => (run()?.id === runId ? (runEvents().at(-1)?.sequence ?? 0) : 0));
		setRunLoading(true);
		setRunError("");
		try {
			const [snapshot, events] = await Promise.all([
				callRun({ action: "get", run_id: runId }),
				callRun({ action: "events", run_id: runId, after_sequence: after, limit: 100 }),
			]);
			if (snapshot.type !== "snapshot" || events.type !== "events") throw new Error("Invalid run response");
			if (current !== runRequest) return;
			setRun(snapshot.value);
			setRunEvents((previous) => (after ? [...previous, ...events.value] : events.value));
		} catch (cause) {
			if (current === runRequest) setRunError(String(cause));
		} finally {
			if (current === runRequest) setRunLoading(false);
		}
	}

	async function openRuns(): Promise<void> {
		const currentPlan = planId();
		if (!currentPlan) return;
		if (run()?.planId !== currentPlan) {
			++runRequest;
			setRuns([]);
			setSelectedRunId(null);
			setRun(null);
			setRunEvents([]);
		}
		setShowRuns(true);
		setRunLoading(true);
		setRunError("");
		try {
			const reply = await callRun({ action: "list_plan_runs", plan_id: currentPlan, limit: 20 });
			if (reply.type !== "runs") throw new Error("Invalid run list response");
			if (currentPlan !== planId() || !showRuns()) return;
			setRuns(reply.value);
			const selected = reply.value.find((item) => item.id === selectedRunId())?.id ?? reply.value[0]?.id ?? null;
			const unchanged = selected === selectedRunId();
			setSelectedRunId(selected);
			if (!selected) {
				setRun(null);
				setRunEvents([]);
				setRunLoading(false);
			} else if (unchanged) await loadRun(selected);
		} catch (cause) {
			setRunError(String(cause));
			setRunLoading(false);
		}
	}

	async function answerRunInput(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const selected = run();
		const attempt = pendingInput();
		const answer = inputAnswer().trim();
		if (!selected || !attempt || !answer) return;
		setRunLoading(true);
		setRunError("");
		try {
			const reply = await callRun({
				action: "command",
				run_id: selected.id,
				command_id: `answer-input:${attempt.id}`,
				expected_sequence: selected.sequence,
				command: { action: "answer_input", attempt_id: attempt.id, answer },
			});
			if (reply.type !== "receipt") throw new Error("Invalid run response");
			setRun(reply.value.snapshot);
			setInputAnswer("");
		} catch (cause) {
			setRunError(String(cause));
		} finally {
			setRunLoading(false);
		}
	}

	async function controlRun(command: Record<string, unknown>): Promise<void> {
		const selected = run();
		if (!selected) return;
		setRunLoading(true);
		setRunError("");
		try {
			const reply = await callRun({
				action: "command",
				run_id: selected.id,
				command_id: crypto.randomUUID(),
				expected_sequence: selected.sequence,
				command,
			});
			if (reply.type !== "receipt") throw new Error("Invalid run response");
			setRun(reply.value.snapshot);
			setRecoveryExecution("");
			setRecoveryActivation("");
			setResolution("");
			await loadRun(selected.id);
		} catch (cause) {
			setRunError(String(cause));
		} finally {
			setRunLoading(false);
		}
	}

	async function openStart(): Promise<void> {
		setBusy(true);
		setError("");
		const selected = storyId();
		try {
			const reply = await invoke<{
				type: string;
				value: { id: string; name: string; kind: string; latestPublishedRevision: number | null }[];
			}>("workflow_definition_action", { project: props.project, action: { action: "list_drafts" } });
			if (reply.type !== "drafts") throw new Error("Invalid workflow response");
			if (selected !== storyId()) return;
			const choices = reply.value.filter(
				(item): item is typeof item & { latestPublishedRevision: number } =>
					item.kind === "story" && item.latestPublishedRevision !== null,
			);
			setPublishedChoices(choices);
			setWorkflowChoice(choices[0]?.id ?? "");
			startRequestId = crypto.randomUUID();
			setStartOpen(true);
		} catch (cause) {
			fail(cause);
		} finally {
			setBusy(false);
		}
	}

	async function startStoryWorkflow(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const selected = selectedStory();
		const workflow = publishedChoices().find((item) => item.id === workflowChoice());
		if (!selected || !workflow) return;
		setBusy(true);
		setError("");
		try {
			const reply = await callRun({
				action: "start_graph",
				target: { type: "story", id: selected.id },
				expected_revision: selected.revision,
				definition_id: workflow.id,
				definition_revision: workflow.latestPublishedRevision,
				request_id: startRequestId,
			});
			if (reply.type !== "snapshot") throw new Error("Invalid run response");
			if (selected.planId !== planId()) return;
			setStartOpen(false);
			setShowDesigner(false);
			setShowRuns(true);
			setRuns((previous) => [reply.value, ...previous.filter((item) => item.id !== reply.value.id)]);
			const unchanged = selectedRunId() === reply.value.id;
			setRun(null);
			setRunEvents([]);
			setSelectedRunId(reply.value.id);
			if (unchanged) await loadRun(reply.value.id);
		} catch (cause) {
			fail(cause);
		} finally {
			setBusy(false);
		}
	}

	createEffect(() => {
		const runId = selectedRunId();
		if (!showRuns() || !runId) return;
		workflowRunSignals.sequence(selectedPlan()?.project ?? props.project, runId);
		workflowRunSignals.resyncRevision();
		void loadRun(runId);
	});

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
					<div class={s.headerActions}>
						<button
							type="button"
							aria-pressed={!showRuns() && !showDesigner()}
							onClick={() => {
								setShowRuns(false);
								setShowDesigner(false);
							}}
						>
							Stories
						</button>
						<button
							type="button"
							disabled={busy() || selectedStory()?.status !== "ready" || !!selectedStory()?.claimSession}
							onClick={() => void openStart()}
						>
							Start story workflow
						</button>
						{/* DEFERRED (2026-10-06) — plan start awaits the slice E dispatch entry point. */}
						<button type="button" disabled title="Plan dispatch is unavailable in this build">
							Start plan workflow · unavailable
						</button>
						<button
							type="button"
							aria-pressed={showRuns()}
							disabled={!planId()}
							onClick={() => {
								setShowDesigner(false);
								void openRuns();
							}}
						>
							Run history
						</button>
						<button
							type="button"
							aria-pressed={showDesigner()}
							onClick={() => {
								setShowRuns(false);
								setShowDesigner(true);
							}}
						>
							Designer
						</button>
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
					</div>
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
				<Show when={startOpen()}>
					<form class={s.form} onSubmit={(event) => void startStoryWorkflow(event)}>
						<p>Start · {selectedStory()?.title}</p>
						<Show
							when={publishedChoices().length > 0}
							fallback={<p>No published story workflow. Publish one in Designer.</p>}
						>
							<label>
								Published workflow
								<select
									value={workflowChoice()}
									onChange={(event) => {
										setWorkflowChoice(event.currentTarget.value);
										startRequestId = crypto.randomUUID();
									}}
								>
									<For each={publishedChoices()}>
										{(workflow) => (
											<option value={workflow.id}>
												{workflow.name} · revision {workflow.latestPublishedRevision}
											</option>
										)}
									</For>
								</select>
							</label>
							<button type="submit" disabled={busy() || !workflowChoice()}>
								Start published workflow
							</button>
						</Show>
						<button type="button" onClick={() => setStartOpen(false)}>
							Dismiss
						</button>
					</form>
				</Show>
				<Show when={showDesigner()}>
					<WorkflowDesigner project={props.project} />
				</Show>
				<Show when={showRuns()}>
					<div class={s.runContent}>
						<aside class={s.runList} aria-label="Plan runs">
							<div class={s.columnHeader}>
								<h3>Runs · {selectedPlan()?.title}</h3>
								<button type="button" onClick={() => void openRuns()}>
									Refresh
								</button>
							</div>
							<Show when={!runLoading() && runs().length === 0}>
								<p class={s.empty}>No workflow runs for this plan.</p>
							</Show>
							<nav class={s.items}>
								<For each={runs()}>
									{(item) => (
										<button
											type="button"
											class={s.item}
											aria-current={selectedRunId() === item.id ? "true" : undefined}
											onClick={() => setSelectedRunId(item.id)}
										>
											<span>{new Date(item.startedMs).toLocaleString()}</span>
											<small>
												{item.status} · {item.stories.length} {item.stories.length === 1 ? "story" : "stories"} · #
												{item.sequence}
											</small>
										</button>
									)}
								</For>
							</nav>
						</aside>
						<section class={s.runDetail} aria-label="Run timeline">
							<Show when={runError()}>
								<p class={s.error} role="alert">
									{runError()}
								</p>
							</Show>
							<Show when={runLoading()}>
								<p class={s.message} role="status">
									Loading run…
								</p>
							</Show>
							<Show when={run()}>
								{(selected) => (
									<>
										<div class={s.detailHeader}>
											<div>
												<span class={s.eyebrow}>Run · {selected().id}</span>
												<h3>{selectedPlan()?.title}</h3>
											</div>
											<span class={s.status} data-status={selected().status}>
												{selected().status.replaceAll("_", " ")}
											</span>
										</div>
										<p class={s.muted}>
											{selected().stories.length} {selected().stories.length === 1 ? "story" : "stories"} ·{" "}
											{selected().attempts.length} {selected().attempts.length === 1 ? "attempt" : "attempts"} ·
											sequence {selected().sequence}
										</p>
										<Show when={pendingInput()}>
											{(attempt) => (
												<form class={s.form} onSubmit={(event) => void answerRunInput(event)}>
													<p>{attempt().report?.inputRequest?.question}</p>
													<label>
														Answer
														<input
															value={inputAnswer()}
															onInput={(event) => setInputAnswer(event.currentTarget.value)}
														/>
													</label>
													<Show when={attempt().report?.inputRequest?.options.length}>
														<small>Suggested: {attempt().report?.inputRequest?.options.join(", ")}</small>
													</Show>
													<button type="submit" disabled={runLoading() || !inputAnswer().trim()}>
														Record answer
													</button>
												</form>
											)}
										</Show>

										<Show when={!selected().graphExecutions?.length}>
											<p class={s.muted}>Legacy run · inspect or cancel only.</p>
										</Show>
										<Show when={selected().status === "running" && !!selected().graphExecutions?.length}>
											<button
												type="button"
												class={s.loadMore}
												disabled={runLoading()}
												onClick={() => void controlRun({ action: "pause" })}
											>
												Pause run
											</button>
										</Show>
										<Show when={selected().status === "running" || selected().status === "paused"}>
											<button
												type="button"
												class={s.loadMore}
												disabled={runLoading()}
												onClick={() => void controlRun({ action: "cancel" })}
											>
												Cancel run
											</button>
										</Show>
										<For each={selected().graphExecutions ?? []}>
											{(graph) => (
												<section aria-label={`Graph ${graph.id}`}>
													<h4>
														{graph.id} · {graph.targetId} · revision {graph.definition.revision}
													</h4>
													<For each={graph.activations}>
														{(activation) => (
															<p>
																{activation.id} · {activation.nodeId} · {activation.state}
															</p>
														)}
													</For>
													<For each={graph.decisions}>
														{(decision) => (
															<p>
																Decision · {decision.activationId} ·{" "}
																{graph.definition.graph.edges[decision.edgeIndex]?.outcome} · {decision.evidence.actor}{" "}
																· {decision.evidence.reason} · {decision.evidence.references.join(", ")}
															</p>
														)}
													</For>
													<For each={graph.loops}>
														{(loop) => (
															<p>
																Loop · {loop.nodeId} · {loop.repeats} repeats
															</p>
														)}
													</For>
													<For each={graph.pauses}>
														{(pause) => (
															<p>
																Pause · {pause.activationId} · {pause.evidence.reason} · resume to {pause.resumeTo} ·{" "}
																{pause.resolution ?? "unresolved"}
															</p>
														)}
													</For>
												</section>
											)}
										</For>
										<Show
											when={selected().status === "paused" && !!selected().graphExecutions?.length && !pendingInput()}
										>
											<form
												class={s.form}
												onSubmit={(event) => {
													event.preventDefault();
													void controlRun({
														action: "resume_graph",
														execution_id: recoveryExecution(),
														activation_id: recoveryActivation(),
														resolution: resolution().trim(),
													});
												}}
											>
												<label>
													Execution
													<select
														value={recoveryExecution()}
														onChange={(event) => {
															setRecoveryExecution(event.currentTarget.value);
															setRecoveryActivation("");
														}}
													>
														<option value="">Select execution</option>
														<For each={selected().graphExecutions ?? []}>
															{(graph) => (
																<option value={graph.id}>
																	{graph.id} · {graph.targetId}
																</option>
															)}
														</For>
													</select>
												</label>
												<label>
													Resume activation
													<select
														value={recoveryActivation()}
														onChange={(event) => setRecoveryActivation(event.currentTarget.value)}
													>
														<option value="">Select activation</option>
														<For
															each={
																selected()
																	.graphExecutions?.find((graph) => graph.id === recoveryExecution())
																	?.activations.filter((activation) => activation.state !== "completed") ?? []
															}
														>
															{(activation) => (
																<option value={activation.id}>
																	{activation.id} · {activation.nodeId} · {activation.state}
																</option>
															)}
														</For>
													</select>
												</label>
												<label>
													Resolution
													<textarea
														value={resolution()}
														onInput={(event) => setResolution(event.currentTarget.value)}
														maxLength={4096}
													/>
												</label>
												<button
													type="submit"
													disabled={
														runLoading() || !recoveryExecution() || !recoveryActivation() || !resolution().trim()
													}
												>
													Resume run
												</button>
											</form>
										</Show>
										<ol class={s.timeline}>
											<For each={runEvents()}>
												{(event) => (
													<li>
														<span class={s.eventSequence}>#{event.sequence}</span>
														<div>
															<span>{event.kind.type.replaceAll("_", " ")}</span>
															<details>
																<summary>Event details</summary>
																<pre>{JSON.stringify(event.kind, null, 2)}</pre>
															</details>
														</div>
														<time>{new Date(event.atMs).toLocaleTimeString()}</time>
													</li>
												)}
											</For>
										</ol>
										<Show when={(runEvents().at(-1)?.sequence ?? 0) < selected().sequence}>
											<button
												type="button"
												class={s.loadMore}
												disabled={runLoading()}
												onClick={() => void loadRun(selected().id)}
											>
												Load more events
											</button>
										</Show>
									</>
								)}
							</Show>
						</section>
					</div>
				</Show>
				<Show when={!showRuns() && !showDesigner()}>
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
				</Show>
			</div>
		</div>
	);
};
