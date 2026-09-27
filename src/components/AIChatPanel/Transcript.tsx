/**
 * The conversation, drawn from the projection `acpTranscript` builds.
 *
 * Nothing here reaches for the store or the client: an entry arrives already
 * folded — chunks joined, tool-call updates merged into the card that opened
 * them, one plan rather than every intermediate copy of it — so this file is
 * only the shape each kind takes on screen.
 */

import { type Component, createMemo, For, type JSX, Match, Show, Switch } from "solid-js";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";
import type { AcpToolCall, AcpToolCallContent } from "../../types/acp";
import { cx } from "../../utils";
import { ContentRenderer } from "../ui/ContentRenderer";
import s from "./AIChatPanel.module.css";

/** Why a turn ended, for the turns that ended without an answer. */
const SETTLEMENTS: Record<string, string> = {
	cancelled: "Turn cancelled.",
	refusal: "The agent refused this turn.",
	max_tokens: "The turn stopped: the answer ran out of room.",
	max_turn_requests: "The turn stopped: it reached its request limit.",
};

function settlement(stopReason: string): string {
	return SETTLEMENTS[stopReason] ?? `Turn ended: ${stopReason}.`;
}

const STATUS_CLASS: Record<string, string> = {
	pending: s.toolCallPending,
	in_progress: s.toolCallPending,
	completed: s.toolCallSuccess,
	failed: s.toolCallFailure,
};

/** The one line a tool-call body is worth: what it touched, or what it said. */
function toolCallDetail(call: AcpToolCall): string {
	const locations = call.locations?.map((location) => location.path) ?? [];
	if (locations.length > 0) return locations.join(", ");
	return (call.content ?? []).map(contentLine).filter(Boolean).join("\n");
}

function contentLine(content: AcpToolCallContent): string {
	if (content.type === "diff") return content.path;
	if (content.type === "terminal") return `terminal ${content.terminalId}`;
	return content.content.type === "text" ? content.content.text : "";
}

const ToolActivity: Component<{ calls: () => AcpToolCall[] }> = (props) => {
	const startedAt = performance.now();
	let finishedAt: number | undefined;
	let observedCount = 0;
	const status = () => {
		const calls = props.calls();
		if (calls.some((call) => call.status === "failed")) return "Failed";
		if (calls.some((call) => call.status === "pending" || call.status === "in_progress" || !call.status)) return "Running";
		return "Completed";
	};
	const duration = () => {
		const calls = props.calls();
		if (calls.length !== observedCount) {
			observedCount = calls.length;
			finishedAt = undefined;
		}
		if (calls.every((call) => call.status === "completed" || call.status === "failed")) finishedAt ??= performance.now();
		const seconds = ((finishedAt ?? performance.now()) - startedAt) / 1000;
		return `${seconds.toFixed(1)}s`;
	};
	return (
		<details class={s.toolActivity}>
			<summary class={s.toolActivitySummary}>
				<span class={cx(s.toolCallStatusDot, status() === "Failed" ? s.toolCallFailure : status() === "Running" ? s.toolCallPending : s.toolCallSuccess)} />
				<span>{props.calls().length} tool {props.calls().length === 1 ? "call" : "calls"}</span>
				<span class={s.toolActivityTitles}>{props.calls().slice(0, 2).map((call) => call.title).join(" · ")}{props.calls().length > 2 ? " · …" : ""}</span>
				<span class={s.toolCallDuration}>{duration()} observed · {status()}</span>
			</summary>
			<div class={s.toolActivityCalls}>
				<For each={props.calls()}>
					{(call) => (
						<details class={s.toolActivityCall}>
							<summary class={s.toolActivityCallSummary}>
								<span class={cx(s.toolCallStatusDot, STATUS_CLASS[call.status ?? "pending"])} />
								<span class={s.toolCallName}>{call.title}</span>
								<span class={s.toolCallDuration}>{call.kind ?? "other"} · {call.status === "failed" ? "Failed" : call.status === "completed" ? "Completed" : "Running"}</span>
							</summary>
							<Show when={toolCallDetail(call)}>
								<div class={s.toolCallBody}>{toolCallDetail(call)}</div>
							</Show>
						</details>
					)}
				</For>
			</div>
		</details>
	);
};

/** Keep the first call as the stable row anchor; later calls belong to it. */
function activityRows(entries: AcpTranscriptEntry[]): { visible: AcpTranscriptEntry[]; calls: Map<string, AcpToolCall[]> } {
	const visible: AcpTranscriptEntry[] = [];
	const calls = new Map<string, AcpToolCall[]>();
	let current: AcpToolCall[] | undefined;
	for (const entry of entries) {
		if (entry.kind === "user" || entry.kind === "settled") current = undefined;
		if (entry.kind === "tool") {
			if (!current) {
				current = [];
				calls.set(entry.id, current);
				visible.push(entry);
			}
			current.push(entry.call);
		} else {
			visible.push(entry);
		}
	}
	return { visible, calls };
}

export interface TranscriptProps {
	entries: () => AcpTranscriptEntry[];
	/** Shown while a turn is running and nothing has streamed back yet. */
	busy: () => boolean;
	emptyMessage: string;
	/** Open questions, drawn at the end of the conversation they belong to. */
	children?: JSX.Element;
}

export const Transcript: Component<TranscriptProps> = (props) => {
	const activity = createMemo(() => activityRows(props.entries()));
	return (
		<div class={s.messageList}>
			<Show when={props.entries().length > 0} fallback={<div class={s.emptyState}>{props.emptyMessage}</div>}>
				<For each={activity().visible}>
					{(entry) => (
						<Switch>
							<Match when={entry.kind === "user" && entry}>
								{(user) => <div class={s.userMsg}>{user().text}</div>}
							</Match>
							<Match when={entry.kind === "agent" && entry}>
								{(agent) => (
									<div class={s.assistantMsg}>
										{/* Incremental: an answer is append-only while it streams, so a
										    tick re-parses the block still being written and not the
										    whole message. */}
										<ContentRenderer content={agent().text} incremental={true} />
									</div>
								)}
							</Match>
							<Match when={entry.kind === "thought" && entry}>
								{(thought) => (
									<details class={s.reasoningDisclosure}>
										<summary class={s.reasoningSummary}>Thinking</summary>
										<div class={s.reasoningBody}>{thought().text}</div>
									</details>
								)}
							</Match>
							<Match when={entry.kind === "tool" && entry}>{(tool) => <ToolActivity calls={() => activity().calls.get(tool().id) ?? []} />}</Match>
							<Match when={entry.kind === "plan" && entry}>
								{(plan) => (
									<div class={s.toolCallCard}>
										<div class={s.toolCallHeader}>
											<span class={s.toolCallName}>Plan</span>
										</div>
										<ul class={s.planList}>
											<For each={plan().entries}>
												{(step) => (
													<li
														class={cx(
															s.planItem,
															step.status === "completed" && s.planItemDone,
															step.status === "in_progress" && s.planItemActive,
														)}
													>
														<span>{step.status === "completed" ? "✓" : "•"}</span>
														<span>{step.content}</span>
													</li>
												)}
											</For>
										</ul>
									</div>
								)}
							</Match>
							<Match when={entry.kind === "settled" && entry}>
								{(ended) => <div class={s.settledNote}>{settlement(ended().stopReason)}</div>}
							</Match>
						</Switch>
					)}
				</For>
			</Show>
			<Show when={props.busy()}>
				<div class={cx(s.assistantMsg, s.thinkingPulse)}>…</div>
			</Show>
			{props.children}
		</div>
	);
};
