import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import { createCodeMirror } from "solid-codemirror";
import { type Accessor, type Component, createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import type { QueuedCommand } from "../../hooks/usePty";
import { anyModalOpen } from "../../stores/modalStack";
import { cx } from "../../utils";
import s from "./ComposePanel.module.css";

/** One request to append text to an already-open Compose panel, e.g. from
 *  triggering a Smart Prompt while Compose is already open. `seq` must
 *  increase on every request — it's what lets the panel tell two consecutive
 *  requests for the same text apart. */
export interface ComposeAppendRequest {
	text: string;
	seq: number;
}

const composeTheme = EditorView.theme(
	{
		"&": {
			width: "100%",
			height: "100%",
			fontSize: "13px",
			background: "var(--bg-primary)",
		},
		".cm-scroller": {
			fontFamily: "var(--font-mono)",
			overflow: "auto",
		},
		".cm-content": {
			caretColor: "var(--accent)",
			padding: "8px 12px",
		},
		".cm-cursor, .cm-dropCursor": {
			borderLeftColor: "var(--accent)",
		},
		"&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
			backgroundColor: "rgba(122, 162, 247, 0.2)",
		},
		".cm-activeLine": {
			backgroundColor: "transparent",
		},
		"&.cm-focused": {
			outline: "none",
		},
	},
	{ dark: true },
);

export interface ComposePanelProps {
	isOpen: Accessor<boolean>;
	initialText: Accessor<string>;
	onClose: () => void;
	onSend: (text: string) => void | Promise<void>;
	/** Hand the text to the backend's idle gate instead of typing it now. */
	onEnqueue: (text: string) => void | Promise<void>;
	/** False for a plain shell: only an agent has a composer worth waiting for. */
	canEnqueue: Accessor<boolean>;
	/** Commands already waiting for this session's next idle window. */
	queuedCount: Accessor<number>;
	onClearQueue: () => void | Promise<void>;
	/** Read the queued commands themselves — only called while the list is open. */
	onLoadQueue: () => Promise<QueuedCommand[]>;
	/** Drop a single queued command by id. */
	onRemoveQueued: (id: number) => void | Promise<void>;
	onTextChange?: (text: string) => void;
	/** Set (with a fresh `seq`) to append text to the doc while already open,
	 *  instead of replacing it — see the `initialText` effect below for why
	 *  triggering a prompt a second time while Compose is open needs a
	 *  separate signal rather than reusing `initialText`. */
	appendRequest?: Accessor<ComposeAppendRequest | null>;
}

export const ComposePanel: Component<ComposePanelProps> = (props) => {
	const { ref, editorView, createExtension } = createCodeMirror({
		onValueChange: (value) => props.onTextChange?.(value),
	});

	createExtension(composeTheme);
	createExtension(drawSelection());
	createExtension(history());
	createExtension(EditorView.lineWrapping);
	createExtension(
		keymap.of([
			// Shift first: CodeMirror matches the more specific binding, so plain
			// Ctrl-Enter keeps sending immediately.
			{
				key: "Shift-Ctrl-Enter",
				run: (view) => {
					const text = view.state.doc.toString().trim();
					if (text && props.canEnqueue()) props.onEnqueue(text);
					return true;
				},
			},
			{
				key: "Ctrl-Enter",
				run: (view) => {
					const text = view.state.doc.toString().trim();
					if (text) props.onSend(text);
					return true;
				},
			},
			{
				key: "Escape",
				run: () => {
					props.onClose();
					return true;
				},
			},
			...defaultKeymap,
			...historyKeymap,
		]),
	);

	// Re-initialise content only when the panel opens — NOT on every keystroke.
	// Tracking initialText() here would re-run on every user keystroke (since
	// onTextChange feeds back into the same signal), causing a 2-RAF delay loop
	// that overwrites the current content with content from ~32ms ago (ghost text).
	createEffect(
		on(props.isOpen, (open) => {
			if (!open) return;
			// Read initialText outside reactive tracking — we only want the value
			// at open time, not to subscribe to further changes while typing.
			const initial = props.initialText();
			let inner = 0;
			const outer = requestAnimationFrame(() => {
				inner = requestAnimationFrame(() => {
					const view = editorView();
					if (!view) return;
					const current = view.state.doc.toString();
					if (current !== initial) {
						view.dispatch({
							changes: { from: 0, to: view.state.doc.length, insert: initial },
							selection: { anchor: initial.length },
						});
					}
					view.focus();
				});
			});
			// Closing (or unmounting) before the second frame lands would otherwise
			// dispatch into a destroyed view.
			onCleanup(() => {
				cancelAnimationFrame(outer);
				if (inner) cancelAnimationFrame(inner);
			});
		}),
	);

	// Append (rather than replace) when a prompt is triggered while Compose is
	// already open. `initialText`'s effect above is edge-triggered on `isOpen`
	// specifically to avoid re-running on every keystroke (see its comment) —
	// so when `isOpen` is already true, setting a new `initialText` there is a
	// no-op and the text never reaches the editor. This is a separate signal
	// precisely so an already-open panel still reacts. `on(..., { defer: true })`
	// skips the initial run so mounting with a null/stale request appends nothing.
	createEffect(
		on(
			() => props.appendRequest?.() ?? null,
			(request) => {
				if (!request?.text) return;
				const view = editorView();
				if (!view) return;
				const end = view.state.doc.length;
				const insertion = end > 0 ? `\n\n${request.text}` : request.text;
				view.dispatch({
					changes: { from: end, to: end, insert: insertion },
					selection: { anchor: end + insertion.length },
				});
				view.focus();
			},
			{ defer: true },
		),
	);

	createEffect(() => {
		if (!props.isOpen()) return;
		// Tracks whichever frame is currently pending so cleanup can cancel it —
		// mirrors the initialText effect above, which cancels its own rAFs for
		// the same reason: a frame firing after the panel closes/unmounts would
		// call into a torn-down view.
		let pendingFrame: number | null = null;
		const handleFocusOut = () => {
			// Deciding purely from `relatedTarget` (as this used to) fires too early:
			// WebKit doesn't populate it for a mousedown-focused button, and even
			// where it's populated the *next* focus target hasn't necessarily
			// settled into `document.activeElement` yet. Deferring the whole
			// decision to the next frame — after focus has actually settled — is
			// what lets a click into an unrelated dialog (e.g. Edit Prompt, opened
			// while Compose stays open) keep its focus instead of losing it back to
			// CodeMirror one frame later.
			pendingFrame = requestAnimationFrame(() => {
				pendingFrame = null;
				if (!props.isOpen()) return;
				if (anyModalOpen()) return;
				const active = document.activeElement;
				// Only reclaim when focus fell through to <body> (or nowhere) — that's
				// the "keystrokes would otherwise vanish" case this exists for. Focus
				// that landed anywhere else (a dialog field, another panel) is left
				// alone; nothing here needs that target's cooperation.
				if (active && active !== document.body) return;
				editorView()?.focus();
			});
		};
		const cmDom = editorView()?.dom;
		cmDom?.addEventListener("focusout", handleFocusOut);
		onCleanup(() => {
			cmDom?.removeEventListener("focusout", handleFocusOut);
			if (pendingFrame !== null) cancelAnimationFrame(pendingFrame);
		});
	});

	// The badge count comes from the cheap lifecycle poll; the texts cost an IPC
	// round-trip, so they are fetched only while the list is expanded — and
	// re-fetched whenever the count moves under it (the queue drains on idle).
	const [queueOpen, setQueueOpen] = createSignal(false);
	const [queueItems, setQueueItems] = createSignal<QueuedCommand[]>([]);

	createEffect(() => {
		if (!queueOpen()) return;
		const count = props.queuedCount();
		if (count === 0) {
			setQueueOpen(false);
			setQueueItems([]);
			return;
		}
		void props.onLoadQueue().then(setQueueItems);
	});

	createEffect(() => {
		if (!props.isOpen()) setQueueOpen(false);
	});

	const currentText = (): string => editorView()?.state.doc.toString().trim() ?? "";

	const handleSend = () => {
		const text = currentText();
		if (text) props.onSend(text);
	};

	const handleEnqueue = () => {
		const text = currentText();
		if (text && props.canEnqueue()) props.onEnqueue(text);
	};

	return (
		<div class={cx(s.panel, props.isOpen() && s.panelOpen)} onMouseDown={(e) => e.stopPropagation()}>
			<div class={s.editor} ref={ref} />
			<Show when={queueOpen() && props.queuedCount() > 0}>
				<div class={s.queueList}>
					<div class={s.queueListHeader}>
						<span>Waiting for the agent to go idle</span>
						<button class={s.queueClearAll} onClick={() => props.onClearQueue()}>
							Clear all
						</button>
					</div>
					<For each={queueItems()}>
						{(item) => (
							<div class={s.queueItem}>
								<span class={s.queueItemText} title={item.text}>
									{item.text}
								</span>
								<button
									class={s.queueItemRemove}
									onClick={() => props.onRemoveQueued(item.id)}
									title="Remove from queue"
									aria-label="Remove from queue"
								>
									<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
										<path d="M4.3 3.3l8.4 8.4-1 1-8.4-8.4zM12.7 4.3l-8.4 8.4-1-1 8.4-8.4z" />
									</svg>
								</button>
							</div>
						)}
					</For>
				</div>
			</Show>
			<div class={s.statusBar}>
				<span>Ctrl+Enter to send &middot; {props.canEnqueue() ? "Shift+Ctrl+Enter to queue · " : ""}Esc to close</span>
				<div class={s.actions}>
					<Show when={props.queuedCount() > 0}>
						<button
							class={cx(s.queueBadge, queueOpen() && s.queueBadgeOpen)}
							onClick={() => setQueueOpen(!queueOpen())}
							title="Waiting for the agent to go idle — click to review"
							aria-expanded={queueOpen()}
						>
							{props.queuedCount()} queued
							<svg
								width="10"
								height="10"
								viewBox="0 0 16 16"
								fill="currentColor"
								aria-hidden="true"
								class={cx(s.queueChevron, queueOpen() && s.queueChevronOpen)}
							>
								<path d="M8 11L3 5h10z" />
							</svg>
						</button>
					</Show>
					<Show when={props.canEnqueue()}>
						<button
							class={s.queueButton}
							onClick={handleEnqueue}
							title="Queue for the next idle moment (Shift+Ctrl+Enter)"
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path d="M2 3h12v2H2zM2 7h12v2H2zM2 11h8v2H2z" />
							</svg>
						</button>
					</Show>
					<button class={s.sendButton} onClick={handleSend} title="Send (Ctrl+Enter)">
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M4 2l10 6-10 6V2z" />
						</svg>
					</button>
				</div>
			</div>
		</div>
	);
};
