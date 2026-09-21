import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import { effectiveActivityState } from "../../utils/activitySnapshot";
import { writeClipboard } from "../../utils/clipboard";
import s from "./StateExplainModal.module.css";

interface RankedSource {
	rank: string;
	source: string;
}
interface EvidenceSnapshot {
	rank: string;
	source: string;
	age_ms: number;
}
interface AgentExplain {
	agent_type: string | null;
	agent_seen_running: boolean;
	hook_instrumented: boolean;
	hook_state_seen: boolean;
	has_ready_screen_adapter: boolean;
}
interface VisibleExplain {
	shell_state: string | null;
	agent_state: string | null;
	agent_state_rung: string;
	awaiting_input: boolean;
	question_confident: boolean;
	choice_prompt_present: boolean;
	background_work: boolean;
	declared_background_work: boolean;
	rate_limited: boolean;
	queued_commands: number;
	turn_epoch: number;
	active_sub_tasks: number;
}
interface EvidenceExplain {
	busy: EvidenceSnapshot | null;
	idle: EvidenceSnapshot | null;
	awaiting: EvidenceSnapshot | null;
	activity_seen: boolean;
	idle_confirmed: boolean;
	shell_is_busy: boolean;
	decide_now: string | null;
}
interface ScreenExplain {
	cached_activity: string;
	screen_ready_pending_since_ms: number | null;
	skipped_by_protocol_authority: boolean;
	no_adapter_for_agent: boolean;
}
interface SilenceExplain {
	last_output_ms_ago: number | null;
	last_chunk_ms_ago: number;
	threshold_ms: number;
	threshold_reason: string;
	remaining_before_fire_ms: number | null;
	startup_settled: boolean;
	last_status_line_ms_ago: number | null;
}
interface NotificationExplain {
	notification_type: string | null;
	has_message: boolean;
	shell_already_idle: boolean;
	confident: boolean | null;
	suppressed: boolean;
	age_ms: number;
}
interface TrailEntryExplain {
	seq: number;
	age_ms: number;
	kind: string;
	rank: string | null;
	source: string | null;
	accepted: boolean;
	forced: boolean;
	outranked_by: RankedSource | null;
}

/** Backend `explain_session_state` response — mirrors `pty/explain.rs`'s
 *  `SessionStateExplain` verbatim, snake_case, on both transports. Some
 *  sections (`epoch_flags`, `holds`) aren't rendered here yet; add fields as
 *  the modal grows rather than widening this interface speculatively. */
export interface SessionStateExplain {
	session_id: string;
	captured_at_ms: number;
	agent: AgentExplain;
	visible: VisibleExplain;
	evidence: EvidenceExplain;
	screen: ScreenExplain;
	silence: SilenceExplain;
	notification: NotificationExplain | null;
	trail: TrailEntryExplain[];
}

function fmtMs(ms: number | null | undefined): string {
	if (ms == null) return "—";
	if (ms < 1000) return `${ms}ms`;
	return `${(ms / 1000).toFixed(1)}s`;
}

function fmtBool(b: boolean): string {
	return b ? "true" : "false";
}

function fmtEvidence(e: EvidenceSnapshot | null): string {
	if (!e) return "none";
	return `${e.rank} · ${e.source} (${fmtMs(e.age_ms)} ago)`;
}

/** Troubleshooting dump: why a session's status badge is what it is.
 *  Fetches `explain_session_state` on mount and renders the chain top to
 *  bottom — evidence/decision, agent & screen bookkeeping, the silence
 *  timer, the last notification classification, then the raw decision
 *  trail — ending with an explicit check of whether the frontend's own
 *  badge (`effectiveActivityState`, computed here from the same props the
 *  real badge uses) agrees with the backend's `agent_state`. That
 *  disagreement is sometimes by design (see `activitySnapshot.ts`), not a
 *  bug, but is exactly the kind of thing worth surfacing rather than
 *  hiding when someone is staring at a badge that looks wrong. */
export const StateExplainModal: Component<{
	sessionId: string;
	shellState: string | null;
	awaitingInput: string | null;
	isRateLimited: boolean;
	agentState: string | null;
	backgroundWork: boolean;
	declaredBackgroundWork: boolean;
	onClose: () => void;
}> = (props) => {
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);
	const [explain, setExplain] = createSignal<SessionStateExplain | null>(null);
	const [copied, setCopied] = createSignal(false);

	onMount(async () => {
		try {
			const result = await invoke<SessionStateExplain | null>("explain_session_state", {
				sessionId: props.sessionId,
			});
			if (!result) {
				setError("Session not found — it may have exited.");
			} else {
				setExplain(result);
			}
		} catch (e) {
			setError(String(e));
			appLogger.error("state-explain", "explain_session_state failed", { error: String(e) });
		} finally {
			setLoading(false);
		}
	});

	const frontendBadge = () =>
		effectiveActivityState(
			props.shellState,
			props.awaitingInput,
			props.isRateLimited,
			props.agentState,
			props.backgroundWork,
			props.declaredBackgroundWork,
		);

	/** Whether the frontend badge and the backend `agent_state` disagree in a
	 *  way that ISN'T one of `effectiveActivityState`'s own documented
	 *  carve-outs. `rate_limited`/`"error"` are orthogonal axes `agent_state`
	 *  has no concept of at all (rate-limiting and API errors aren't part of
	 *  the busy/idle/awaiting ladder), so they must never read as a
	 *  disagreement — code review caught that every rate-limited session
	 *  previously showed a false-positive mismatch banner. The remaining
	 *  carve-out is idle-with-background-work collapsing "working" to "idle"
	 *  for a ready composer. */
	const disagrees = (ex: SessionStateExplain): boolean => {
		const fe = frontendBadge();
		if (fe === "rate_limited" || fe === "error") return false;
		// A plain shell has no agent_type, so agent_state is legitimately
		// null (see agent_state_rung: "no_agent_type") while the frontend
		// badge still reports a normal shell idle/working state — the two
		// axes aren't comparable here at all, not disagreeing. Without this,
		// every non-agent terminal would show the mismatch banner.
		if (ex.visible.agent_state_rung === "no_agent_type") return false;
		const be = ex.visible.agent_state;
		if (fe === be) return false;
		if (fe === "idle" && be === "working" && ex.visible.background_work) return false;
		return true;
	};

	let copiedTimeout: ReturnType<typeof setTimeout> | undefined;
	onCleanup(() => clearTimeout(copiedTimeout));

	const handleCopy = async () => {
		const ex = explain();
		if (!ex) return;
		await writeClipboard(JSON.stringify(ex, null, 2));
		setCopied(true);
		clearTimeout(copiedTimeout);
		copiedTimeout = setTimeout(() => setCopied(false), 1500);
	};

	registerModal(props.onClose);

	return (
		<div class={s.overlay} onClick={props.onClose}>
			<div class={s.modal} onClick={(e) => e.stopPropagation()}>
				<div class={s.header}>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
						<path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1Zm0 1.5a5.5 5.5 0 1 1 0 11 5.5 5.5 0 0 1 0-11ZM7.25 7h1.5v5h-1.5V7Zm0-2.5h1.5V6h-1.5V4.5Z" />
					</svg>
					<span class={s.title}>Explain state — {props.sessionId.slice(0, 8)}</span>
					<button type="button" class={s.copyBtn} onClick={handleCopy} disabled={!explain()}>
						{copied() ? "Copied" : "Copy as JSON"}
					</button>
					<button type="button" class={s.close} onClick={props.onClose} title="Close">
						&times;
					</button>
				</div>

				<div class={s.body}>
					<Show when={loading()}>
						<div class={s.loading}>
							<span class={s.spinner} />
							Loading…
						</div>
					</Show>
					<Show when={error()}>{(msg) => <div class={s.error}>{msg()}</div>}</Show>
					<Show when={explain()}>
						{(exAccessor) => {
							const ex = exAccessor();
							const mismatch = disagrees(ex);
							return (
								<>
									<Show when={mismatch}>
										<div class={s.banner}>
											Frontend badge (<strong>{frontendBadge()}</strong>) disagrees with backend{" "}
											<code>agent_state</code> (<strong>{ex.visible.agent_state ?? "null"}</strong>). Check{" "}
											<code>agent_state_rung</code> below — this can be by design, but is worth double-checking.
										</div>
									</Show>

									<div class={s.section}>
										<div class={s.sectionTitle}>Visible</div>
										<dl class={s.grid}>
											<dt>shell_state</dt>
											<dd>{ex.visible.shell_state ?? "—"}</dd>
											<dt>agent_state</dt>
											<dd>{ex.visible.agent_state ?? "—"}</dd>
											<dt>agent_state_rung</dt>
											<dd>{ex.visible.agent_state_rung}</dd>
											<dt>frontend badge</dt>
											<dd class={mismatch ? s.badgeMismatch : undefined}>{frontendBadge()}</dd>
											<dt>awaiting_input</dt>
											<dd>
												{fmtBool(ex.visible.awaiting_input)}
												{ex.visible.choice_prompt_present ? " (choice prompt)" : ""}
												{ex.visible.awaiting_input && !ex.visible.question_confident ? " (low-confidence)" : ""}
											</dd>
											<dt>background_work / declared</dt>
											<dd>
												{fmtBool(ex.visible.background_work)} / {fmtBool(ex.visible.declared_background_work)}
											</dd>
											<dt>rate_limited</dt>
											<dd>
												{fmtBool(ex.visible.rate_limited)}
												{ex.visible.rate_limited
													? ' — orthogonal to agent_state; frontend badge always reads "rate_limited" here, not a mismatch'
													: ""}
											</dd>
											<dt>turn_epoch</dt>
											<dd>{ex.visible.turn_epoch}</dd>
										</dl>
									</div>

									<div class={s.section}>
										<div class={s.sectionTitle}>Evidence &amp; decision</div>
										<dl class={s.grid}>
											<dt>busy</dt>
											<dd>{fmtEvidence(ex.evidence.busy)}</dd>
											<dt>idle</dt>
											<dd>{fmtEvidence(ex.evidence.idle)}</dd>
											<dt>awaiting</dt>
											<dd>{fmtEvidence(ex.evidence.awaiting)}</dd>
											<dt>decide_now</dt>
											<dd>{ex.evidence.decide_now ?? "none (no transition proposed)"}</dd>
											<dt>idle_confirmed</dt>
											<dd>{fmtBool(ex.evidence.idle_confirmed)}</dd>
										</dl>
									</div>

									<div class={s.section}>
										<div class={s.sectionTitle}>Agent &amp; screen</div>
										<dl class={s.grid}>
											<dt>agent_type</dt>
											<dd>{ex.agent.agent_type ?? "—"}</dd>
											<dt>hook_instrumented / seen</dt>
											<dd>
												{fmtBool(ex.agent.hook_instrumented)} / {fmtBool(ex.agent.hook_state_seen)}
											</dd>
											<dt>has_ready_screen_adapter</dt>
											<dd class={!ex.agent.has_ready_screen_adapter ? s.badgeMismatch : undefined}>
												{fmtBool(ex.agent.has_ready_screen_adapter)}
												{!ex.agent.has_ready_screen_adapter
													? " — this agent can never go idle from a screen adapter alone"
													: ""}
											</dd>
											<dt>cached screen activity</dt>
											<dd>{ex.screen.cached_activity}</dd>
											<dt>skipped_by_protocol_authority</dt>
											<dd>{fmtBool(ex.screen.skipped_by_protocol_authority)}</dd>
										</dl>
									</div>

									<div class={s.section}>
										<div class={s.sectionTitle}>Silence timer</div>
										<dl class={s.grid}>
											<dt>last_output</dt>
											<dd>{fmtMs(ex.silence.last_output_ms_ago)} ago</dd>
											<dt>threshold</dt>
											<dd>
												{fmtMs(ex.silence.threshold_ms)} ({ex.silence.threshold_reason})
											</dd>
											<dt>remaining_before_fire</dt>
											<dd>{fmtMs(ex.silence.remaining_before_fire_ms)}</dd>
										</dl>
									</div>

									<Show when={ex.notification}>
										{(nAccessor) => {
											const n = nAccessor();
											return (
												<div class={s.section}>
													<div class={s.sectionTitle}>Last notification classification</div>
													<dl class={s.grid}>
														<dt>notification_type</dt>
														<dd>{n.notification_type ?? "—"}</dd>
														<dt>confident</dt>
														<dd>{n.confident === null ? "—" : fmtBool(n.confident)}</dd>
														<dt>suppressed</dt>
														<dd>{fmtBool(n.suppressed)}</dd>
														<dt>age</dt>
														<dd>{fmtMs(n.age_ms)} ago</dd>
													</dl>
												</div>
											);
										}}
									</Show>

									<div class={s.section}>
										<div class={s.sectionTitle}>Decision trail ({ex.trail.length})</div>
										<div class={s.trailList}>
											<For each={ex.trail}>
												{(entry) => (
													<div class={`${s.trailEntry} ${!entry.accepted ? s.rejected : ""}`}>
														<span class={s.trailAge}>{fmtMs(entry.age_ms)} ago</span>
														<span class={s.trailKind}>{entry.kind}</span>
														<span class={s.trailDetail}>
															{entry.rank ?? ""} {entry.source ?? ""}
															{entry.forced ? " (forced)" : ""}
															{!entry.accepted && entry.outranked_by
																? ` — rejected, outranked by ${entry.outranked_by.rank} ${entry.outranked_by.source}`
																: ""}
														</span>
													</div>
												)}
											</For>
										</div>
									</div>
								</>
							);
						}}
					</Show>
				</div>

				<div class={s.footer}>captured {new Date().toLocaleTimeString()}</div>
			</div>
		</div>
	);
};
