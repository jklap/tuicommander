/**
 * The conversation, drawn from the projection `acpTranscript` builds.
 *
 * Nothing here reaches for the store or the client: an entry arrives already
 * folded — chunks joined, tool-call updates merged into the card that opened
 * them, one plan rather than every intermediate copy of it — so this file is
 * only the shape each kind takes on screen.
 */

import { type Component, createEffect, createMemo, createSignal, For, type JSX, Match, Show, Switch } from "solid-js";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";
import { appLogger } from "../../stores/appLogger";
import type { AcpToolCall, AcpToolCallContent } from "../../types/acp";
import { cx } from "../../utils";
import { writeClipboard } from "../../utils/clipboard";
import { handleOpenUrl } from "../../utils/openUrl";
import { filePathRegex, matchWebUrls } from "../Terminal/linkProvider";
import { ContentRenderer } from "../ui/ContentRenderer";
import s from "./AIChatPanel.module.css";
import { projectChatProtocolText } from "./protocolText";

/** Why a turn ended, for the turns that ended without an answer. */
const SETTLEMENTS: Record<string, string> = {
	empty: "Turn ended without a reply.",
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

function toolName(title: string): string {
	return title.split(/\s+-lc\s+|\s+-c\s+/, 1)[0];
}

const ToolActivity: Component<{ calls: () => AcpToolCall[] }> = (props) => {
	const startedAt = performance.now();
	let finishedAt: number | undefined;
	let observedCount = 0;
	const status = () => {
		const calls = props.calls();
		if (calls.some((call) => call.status === "failed")) return "Failed";
		if (calls.some((call) => call.status === "pending" || call.status === "in_progress" || !call.status))
			return "Running";
		return "Completed";
	};
	const duration = () => {
		const calls = props.calls();
		if (calls.length !== observedCount) {
			observedCount = calls.length;
			finishedAt = undefined;
		}
		if (calls.every((call) => call.status === "completed" || call.status === "failed"))
			finishedAt ??= performance.now();
		const seconds = ((finishedAt ?? performance.now()) - startedAt) / 1000;
		return `${seconds.toFixed(1)}s`;
	};
	return (
		<details class={s.toolActivity}>
			<summary class={s.toolActivitySummary}>
				<span
					class={cx(
						s.toolCallStatusDot,
						status() === "Failed" ? s.toolCallFailure : status() === "Running" ? s.toolCallPending : s.toolCallSuccess,
					)}
				/>
				<span class={s.toolCallCount}>
					{props.calls().length} tool {props.calls().length === 1 ? "call" : "calls"}
				</span>
				<span class={s.toolActivityTitles}>
					{props
						.calls()
						.slice(0, 2)
						.map((call) => toolName(call.title))
						.join(" · ")}
					{props.calls().length > 2 ? " · …" : ""}
				</span>
				<span class={s.toolCallDuration}>
					{duration()} observed · {status()}
				</span>
			</summary>
			<div class={s.toolActivityCalls}>
				<For each={props.calls()}>
					{(call) => (
						<details class={s.toolActivityCall}>
							<summary class={s.toolActivityCallSummary}>
								<span class={cx(s.toolCallStatusDot, STATUS_CLASS[call.status ?? "pending"])} />
								<span class={s.toolCallName}>{call.title}</span>
								<span class={s.toolCallDuration}>
									{call.kind ?? "other"} ·{" "}
									{call.status === "failed" ? "Failed" : call.status === "completed" ? "Completed" : "Running"}
								</span>
							</summary>
							<Show when={toolCallDetail(call)}>
								<div class={s.toolCallBody}>
									{toolCallDetail(call)}
									<CopyButton label="Copy tool output" text={toolCallDetail(call)} />
								</div>
							</Show>
						</details>
					)}
				</For>
			</div>
		</details>
	);
};

/** Keep the first call as the stable row anchor; later calls belong to it. */
function activityRows(entries: AcpTranscriptEntry[]): {
	visible: AcpTranscriptEntry[];
	calls: Map<string, AcpToolCall[]>;
} {
	const visible: AcpTranscriptEntry[] = [];
	const calls = new Map<string, AcpToolCall[]>();
	let current: AcpToolCall[] | undefined;
	for (const entry of entries) {
		if (entry.kind === "user" || entry.kind === "settled" || entry.kind === "failed") current = undefined;
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
	onOpenFile?: (href: string) => void;
	onClear?: () => void;
	onSuggestion: (text: string) => void;
	/** Open questions, drawn at the end of the conversation they belong to. */
	children?: JSX.Element;
}

const CopyButton: Component<{ label: string; text: string }> = (props) => (
	<button
		type="button"
		class={s.copyAction}
		aria-label={props.label}
		onClick={() => void writeClipboard(props.text).catch((error) => appLogger.error("ai-chat", "Copy failed", error))}
	>
		Copy
	</button>
);

const LinkedPlainText: Component<{ text: string; onOpenFile?: (href: string) => void }> = (props) => {
	const parts = createMemo(() => {
		const source = props.text;
		const links = matchWebUrls(source).map((url) => ({ start: url.index, text: url.text, web: true }));
		for (const match of source.matchAll(filePathRegex())) {
			links.push({ start: match.index + match[0].indexOf(match[1]), text: match[1], web: false });
		}
		links.sort((a, b) => a.start - b.start);
		const segments: { text: string; web?: boolean; file?: boolean }[] = [];
		let end = 0;
		for (const link of links) {
			if (link.start < end) continue;
			segments.push({ text: source.slice(end, link.start) });
			segments.push({ text: link.text, web: link.web, file: !link.web });
			end = link.start + link.text.length;
		}
		segments.push({ text: source.slice(end) });
		return segments;
	});
	return (
		<For each={parts()}>
			{(part) =>
				part.web ? (
					<a
						href={part.text}
						data-tuic-href={part.text}
						onClick={(event) => {
							event.preventDefault();
							handleOpenUrl(part.text);
						}}
					>
						{part.text}
					</a>
				) : part.file ? (
					<a
						href={part.text}
						onClick={(event) => {
							event.preventDefault();
							props.onOpenFile?.(part.text);
						}}
					>
						{part.text}
					</a>
				) : (
					part.text
				)
			}
		</For>
	);
};

export const Transcript: Component<TranscriptProps> = (props) => {
	const activity = createMemo(() => activityRows(props.entries()));
	const [finding, setFinding] = createSignal(false);
	const [query, setQuery] = createSignal("");
	let container: HTMLDivElement | undefined;
	let searchInput: HTMLInputElement | undefined;
	let matchIndex = -1;
	let stickToBottom = true;
	const onScroll = () => {
		if (!container) return;
		stickToBottom = container.scrollHeight - container.clientHeight - container.scrollTop <= 24;
	};
	createEffect(() => {
		props.entries();
		props.busy();
		queueMicrotask(() => {
			if (container && stickToBottom) container.scrollTop = container.scrollHeight;
		});
	});
	const findNext = () => {
		if (!container || !query()) return;
		const needle = query().toLocaleLowerCase();
		const matches: Range[] = [];
		const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
		while (walker.nextNode()) {
			const node = walker.currentNode;
			if (node.parentElement?.closest("button, input, ." + s.findBar)) continue;
			const text = node.textContent?.toLocaleLowerCase() ?? "";
			for (let at = text.indexOf(needle); at >= 0; at = text.indexOf(needle, at + needle.length)) {
				const range = document.createRange();
				range.setStart(node, at);
				range.setEnd(node, at + needle.length);
				matches.push(range);
			}
		}
		if (!matches.length) return;
		matchIndex = (matchIndex + 1) % matches.length;
		const selection = window.getSelection();
		selection?.removeAllRanges();
		selection?.addRange(matches[matchIndex]);
		matches[matchIndex].startContainer.parentElement?.scrollIntoView?.({ block: "center" });
	};
	const onKeyDown = (event: KeyboardEvent) => {
		if (!(event.metaKey || event.ctrlKey) || event.shiftKey || event.altKey) return;
		if (event.target === searchInput) return;
		const key = event.key.toLowerCase();
		if (key === "a" && container) {
			event.preventDefault();
			event.stopPropagation();
			const range = document.createRange();
			range.selectNodeContents(container);
			window.getSelection()?.removeAllRanges();
			window.getSelection()?.addRange(range);
		} else if (key === "c" && container) {
			const selection = window.getSelection();
			if (!selection?.rangeCount || !container.contains(selection.getRangeAt(0).commonAncestorContainer)) return;
			const selected = selection.toString();
			if (!selected) return;
			event.preventDefault();
			event.stopPropagation();
			void writeClipboard(selected).catch((error) => appLogger.error("ai-chat", "Copy failed", error));
		} else if (key === "f") {
			event.preventDefault();
			event.stopPropagation();
			setFinding(true);
			queueMicrotask(() => searchInput?.focus());
		} else if (key === "k") {
			event.preventDefault();
			event.stopPropagation();
			props.onClear?.();
		}
	};
	return (
		<div
			class={s.messageList}
			ref={container}
			aria-label="Chat transcript"
			tabIndex={0}
			onKeyDown={onKeyDown}
			onScroll={onScroll}
		>
			<Show when={finding()}>
				<div class={s.findBar}>
					<input
						ref={searchInput}
						aria-label="Find in chat"
						value={query()}
						onInput={(event) => {
							setQuery(event.currentTarget.value);
							matchIndex = -1;
						}}
						onKeyDown={(event) => {
							if (event.key === "Enter") {
								event.preventDefault();
								findNext();
							}
							if (event.key === "Escape") setFinding(false);
						}}
					/>
					<button type="button" onClick={findNext}>
						Next
					</button>
					<button type="button" aria-label="Close chat search" onClick={() => setFinding(false)}>
						<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
							<path d="M2.8 2l3.2 3.2L9.2 2l.8.8L6.8 6l3.2 3.2-.8.8L6 6.8 2.8 10l-.8-.8L5.2 6 2 2.8z" />
						</svg>
					</button>
				</div>
			</Show>
			<Show when={props.entries().length > 0} fallback={<div class={s.emptyState}>{props.emptyMessage}</div>}>
				<For each={activity().visible}>
					{(entry) => (
						<Switch>
							<Match when={entry.kind === "user" && entry}>
								{(user) => (
									<div class={s.userMsg}>
										<LinkedPlainText text={user().text} onOpenFile={props.onOpenFile} />
										<CopyButton label="Copy user message" text={user().text} />
									</div>
								)}
							</Match>
							<Match when={entry.kind === "agent" && entry}>
								{(agent) => {
									const projected = createMemo(() => projectChatProtocolText(agent().text));
									return (
										<div class={s.assistantMsg}>
											<Show when={projected().intent}>
												{(intent) => (
													<div class={s.agentIntent} aria-label="Agent intent">
														<span>{intent().title ?? "Status"}</span>
														{intent().text}
													</div>
												)}
											</Show>
											<Show when={projected().body}>
												<ContentRenderer
													content={projected().body}
													incremental={true}
													onLinkClick={props.onOpenFile}
													autoLinkFiles={true}
													onCodeCopy={(text) =>
														void writeClipboard(text).catch((error) => appLogger.error("ai-chat", "Copy failed", error))
													}
												/>
											</Show>
											<CopyButton label="Copy assistant message" text={agent().text} />
											<Show when={projected().suggestions.length > 0}>
												<div class={s.suggestedReplies} aria-label="Suggested replies">
													<For each={projected().suggestions}>
														{(item) => (
															<button type="button" onClick={() => props.onSuggestion(item)}>
																{item}
															</button>
														)}
													</For>
												</div>
											</Show>
										</div>
									);
								}}
							</Match>
							<Match when={entry.kind === "thought" && entry}>
								{(thought) => (
									<details class={s.reasoningDisclosure}>
										<summary class={s.reasoningSummary}>Thinking</summary>
										<div class={s.reasoningBody}>{thought().text}</div>
									</details>
								)}
							</Match>
							<Match when={entry.kind === "tool" && entry}>
								{(tool) => <ToolActivity calls={() => activity().calls.get(tool().id) ?? []} />}
							</Match>
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
							<Match when={entry.kind === "failed" && entry}>
								{(failed) => <div class={s.settledNote}>{failed().message}</div>}
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
