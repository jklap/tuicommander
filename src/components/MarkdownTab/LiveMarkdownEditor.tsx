import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { search, searchKeymap } from "@codemirror/search";
import { EditorState } from "@codemirror/state";
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import { type Component, createEffect, createSignal, on, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { generateTweakCommentId, OverlappingCommentError } from "../../utils/tweakComments";
import { codeEditorTheme } from "../CodeEditorPanel/theme";
import s from "./LiveMarkdownEditor.module.css";
import { addTweakCommentAtSelection, liveLineSeparator, liveMarkdown, loadMarkdownLanguage } from "./liveMarkdown";

export interface LiveMarkdownEditorProps {
	/** Document as stored on disk. The editor saves exactly what is typed, nothing normalised. */
	content: string;
	onSave: (text: string) => Promise<boolean>;
	/** Current on-disk text; a save is refused when it differs from what this editor loaded. */
	readDisk: () => Promise<string>;
	onDirtyChange?: (dirty: boolean) => void;
}

/** CodeMirror editor over the raw markdown; marks are hidden by decoration, never rewritten. */
export const LiveMarkdownEditor: Component<LiveMarkdownEditorProps> = (props) => {
	let host: HTMLDivElement | undefined;
	let view: EditorView | undefined;
	let baseline = props.content;
	const [hasSelection, setHasSelection] = createSignal(false);
	const [composing, setComposing] = createSignal(false);
	const [draft, setDraft] = createSignal("");

	const [dirty, setDirty] = createSignal(false);
	const markDirty = (value: boolean) => {
		setDirty(value);
		props.onDirtyChange?.(value);
	};

	/** Disk text that differs from `baseline`: someone else wrote the file while this buffer was open. */
	const [conflict, setConflict] = createSignal<string | null>(null);

	const commit = async () => {
		if (!view) return;
		const text = view.state.sliceDoc();
		if (!(await props.onSave(text))) return;
		baseline = text;
		setConflict(null);
		markDirty(false);
	};

	const save = async () => {
		if (!view) return;
		let disk: string;
		try {
			disk = await props.readDisk();
		} catch (err) {
			appLogger.error("app", "live save: cannot read the file to check for external changes", err);
			toastsStore.add(t("markdownTab.liveReadFailed", "Couldn't save"), String(err), "error");
			return;
		}
		if (disk !== baseline) {
			setConflict(disk);
			return;
		}
		await commit();
	};

	const reloadFromDisk = () => {
		const disk = conflict();
		if (!view || disk === null) return;
		baseline = disk;
		view.setState(createState(disk));
		setConflict(null);
		markDirty(false);
	};

	const createState = (doc: string) =>
		EditorState.create({
			doc,
			extensions: [
				loadMarkdownLanguage(),
				liveLineSeparator(doc),
				liveMarkdown(),
				codeEditorTheme,
				history(),
				drawSelection(),
				search({ top: true }),
				EditorView.lineWrapping,
				keymap.of([
					{
						key: "Mod-s",
						run: () => {
							void save();
							return true;
						},
					},
					...searchKeymap,
					...historyKeymap,
					...defaultKeymap,
				]),
				EditorView.updateListener.of((update) => {
					if (update.selectionSet) setHasSelection(!update.state.selection.main.empty);
					if (update.docChanged) markDirty(update.state.sliceDoc() !== baseline);
				}),
			],
		});

	onMount(() => {
		if (!host) return;
		view = new EditorView({ state: createState(props.content), parent: host });
		onCleanup(() => view?.destroy());
	});

	// A disk change lands in a clean buffer; a dirty buffer keeps the user's text and the save guard handles it.
	createEffect(
		on(
			() => props.content,
			(incoming) => {
				if (!view || incoming === baseline || view.state.sliceDoc() !== baseline) return;
				baseline = incoming;
				view.setState(createState(incoming));
			},
			{ defer: true },
		),
	);

	const addComment = () => {
		const body = draft().trim();
		if (!view || !body) return;
		try {
			addTweakCommentAtSelection(view, {
				id: generateTweakCommentId(),
				comment: body,
				createdAt: new Date().toISOString(),
			});
			setDraft("");
			setComposing(false);
			view.focus();
		} catch (err) {
			appLogger.error("app", "live addTweakComment failed", err);
			toastsStore.add(
				t("markdownTab.commentOverlap", "Couldn't add comment"),
				err instanceof OverlappingCommentError
					? t(
							"markdownTab.commentOverlapMsg",
							"That text already has a comment. Click the highlight to edit it, or select different text.",
						)
					: String(err),
				"error",
			);
		}
	};

	return (
		<div class={s.root}>
			<div class={s.toolbar}>
				<Show when={hasSelection() && !composing()}>
					<button
						type="button"
						class={s.btn}
						onMouseDown={(ev) => ev.preventDefault()}
						onClick={() => setComposing(true)}
					>
						{t("markdownTab.addComment", "Comment")}
					</button>
				</Show>
				<Show when={composing()}>
					<input
						class={s.input}
						value={draft()}
						placeholder={t("markdownTab.commentPlaceholder", "Comment on the selection")}
						onInput={(ev) => setDraft(ev.currentTarget.value)}
						onKeyDown={(ev) => {
							if (ev.key === "Enter") addComment();
							if (ev.key === "Escape") setComposing(false);
						}}
					/>
					<button type="button" class={s.btn} onClick={addComment}>
						{t("markdownTab.addCommentConfirm", "Add")}
					</button>
					<button type="button" class={s.btn} onClick={() => setComposing(false)}>
						{t("markdownTab.addCommentCancel", "Cancel")}
					</button>
				</Show>
				<Show when={dirty()}>
					<button type="button" class={s.btn} style={{ "margin-left": "auto" }} onClick={() => void save()}>
						{t("markdownTab.saveLive", "Save")}
					</button>
				</Show>
			</div>
			<Show when={conflict() !== null}>
				<div class={s.conflict} role="alert">
					<span>
						{t(
							"markdownTab.liveConflict",
							"This file changed on disk while you were editing. Saving would overwrite those changes.",
						)}
					</span>
					<button type="button" class={s.btn} onClick={reloadFromDisk}>
						{t("markdownTab.liveReload", "Reload")}
					</button>
					<button type="button" class={s.btn} onClick={() => void commit()}>
						{t("markdownTab.liveOverwrite", "Overwrite")}
					</button>
				</div>
			</Show>
			<div ref={host} class={s.host} />
		</div>
	);
};
