import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { ensureSyntaxTree, type LanguageSupport, syntaxTree } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import {
	Annotation,
	type Range as CmRange,
	EditorState,
	type Extension,
	Prec,
	StateEffect,
	StateField,
	Transaction,
	type TransactionSpec,
} from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, WidgetType } from "@codemirror/view";
import type { SyntaxNode } from "@lezer/common";
import {
	findSourceMatch,
	findTweakSyntax,
	insertTweakComment,
	type TweakComment,
	type TweakInlineSpan,
	type TweakSyntaxRegion,
} from "../../utils/tweakComments";

export interface Range {
	from: number;
	to: number;
}

export interface DocEdit {
	from: number;
	to: number;
	insert: string;
}

/** Above this size the editor is plain source: every decoration pass walks the whole tree. */
const LIVE_MAX_CHARS = 500 * 1024;

/** GFM markdown with nested code-block languages, so fenced code keeps its highlighting. */
export function loadMarkdownLanguage(): LanguageSupport {
	return markdown({ base: markdownLanguage, codeLanguages: languages });
}

/** The text outside tweak syntax. The viewer's writer emits LF inside comments whatever the
 *  file uses, so those bytes say nothing about the file's own line endings. */
function withoutTweakSyntax(text: string): string {
	if (!text.includes("<!--tweak") && !text.includes("<!-- tweak-comments")) return text;
	const { spans, standalone } = findTweakSyntax(text);
	const regions = [...spans.flatMap((s) => [s.begin, s.end]), ...standalone].sort((a, b) => b.from - a.from);
	let rest = text;
	for (const r of regions) rest = rest.slice(0, r.from) + rest.slice(r.to);
	return rest;
}

/**
 * True when Live mode can save the file byte for byte. CodeMirror keeps one line
 * separator per document, so mixed endings (or a lone CR) would be rewritten.
 */
export function liveModeSupported(text: string): boolean {
	if (text.length > LIVE_MAX_CHARS) return false;
	const rest = withoutTweakSyntax(text);
	if (/\r(?!\n)/.test(rest)) return false;
	return !(rest.includes("\r\n") && /(?<!\r)\n/.test(rest));
}

/** Pin the separator to the file's own so `state.sliceDoc()` reproduces it. Also stops CM
 *  from splitting on U+2028/U+2029, which it would otherwise turn into newlines. */
export function liveLineSeparator(text: string): Extension {
	return EditorState.lineSeparator.of(withoutTweakSyntax(text).includes("\r\n") ? "\r\n" : "\n");
}

interface LiveValue {
	decorations: DecorationSet;
	/** Markdown marks currently hidden (heading, emphasis, code, link syntax). */
	marks: Range[];
	/** Tweak syntax, always hidden and atomic. */
	tweakRegions: Range[];
	tweakAtomic: DecorationSet;
	spans: TweakInlineSpan[];
	standalone: TweakSyntaxRegion[];
}

const hide = Decoration.replace({});
const HEADING = /^ATXHeading([1-6])$/;
const TASK_MARK = /^\[([ xX~])\](?=\s|$)/;
const BULLETS = ["\u2022", "\u25CB", "\u25AA"];
/** Preview list padding, in em per nesting level. */
const LIST_INDENT_EM = 2;

/** Whether the editor has focus. Off focus nothing is "the cursor line", so every mark hides. */
export const setLiveFocus = StateEffect.define<boolean>();
const focusField = StateField.define<boolean>({
	create: () => true,
	update: (value, tr) => tr.effects.reduce((v, e) => (e.is(setLiveFocus) ? e.value : v), value),
});

class BulletWidget extends WidgetType {
	constructor(readonly glyph: string) {
		super();
	}
	eq(other: BulletWidget) {
		return other.glyph === this.glyph;
	}
	toDOM() {
		const el = document.createElement("span");
		el.className = this.glyph === BULLETS[0] ? "cm-live-bullet" : "cm-live-bullet cm-live-bullet-nested";
		el.textContent = this.glyph;
		return el;
	}
}

/** Source spelling of the next state, cycling like the preview: [ ] -> [x] -> [~] -> [ ]. */
const NEXT_MARK: Record<string, string> = { " ": "x", x: "~", X: "~", "~": " " };

class CheckboxWidget extends WidgetType {
	/** `from` is the `[`; it belongs to `eq` so a reused DOM node never carries a stale position. */
	constructor(
		readonly mark: string,
		readonly from: number,
	) {
		super();
	}
	eq(other: CheckboxWidget) {
		return other.mark === this.mark && other.from === this.from;
	}
	toDOM(view: EditorView) {
		const el = document.createElement("input");
		el.type = "checkbox";
		el.className = "cm-live-checkbox";
		el.checked = this.mark !== " " && this.mark !== "~";
		el.indeterminate = this.mark === "~";
		el.addEventListener("mousedown", (ev) => ev.preventDefault());
		el.addEventListener("click", (ev) => {
			ev.preventDefault();
			const at = this.from + 1;
			const current = view.state.sliceDoc(at, at + 1);
			view.dispatch({
				changes: { from: at, to: at + 1, insert: NEXT_MARK[current] ?? current },
				userEvent: "input.checkbox",
			});
		});
		return el;
	}
	ignoreEvent() {
		return true;
	}
}

/** Lines (1-based, inclusive) touched by any selection range. */
function selectedLineSpans(state: EditorState): Range[] {
	return state.selection.ranges.map((r) => ({
		from: state.doc.lineAt(r.from).number,
		to: state.doc.lineAt(r.to).number,
	}));
}

function buildLive(state: EditorState): LiveValue {
	const doc = state.doc;
	const source = doc.toString();
	const { spans, standalone } = findTweakSyntax(source);
	const tweakRegions: Range[] = [...spans.flatMap((s) => [s.begin, s.end]), ...standalone].sort(
		(a, b) => a.from - b.from,
	);
	const tweakAtomic = Decoration.set(tweakRegions.map((r) => hide.range(r.from, r.to)));

	const decos: CmRange<Decoration>[] = tweakRegions.map((r) => hide.range(r.from, r.to));
	for (const s of spans) {
		if (s.highlight.to > s.highlight.from) {
			decos.push(
				Decoration.mark({ class: "tweak-highlight", attributes: { title: s.comment } }).range(
					s.highlight.from,
					s.highlight.to,
				),
			);
		}
	}

	const marks: Range[] = [];
	const selected = state.field(focusField) ? selectedLineSpans(state) : [];
	const revealed = (from: number, to: number) => {
		const a = doc.lineAt(from).number;
		const b = doc.lineAt(to).number;
		return selected.some((l) => l.from <= b && a <= l.to);
	};
	const overlapsTweak = (from: number, to: number) => tweakRegions.some((r) => from < r.to && r.from < to);
	/** A line that starts where a hidden multi-line region ends is drawn inside the region's first line. */
	const lineStart = (pos: number) => {
		const region = tweakRegions.find((r) => r.from < pos && pos <= r.to);
		return region ? doc.lineAt(region.from).from : pos;
	};
	const hideMark = (from: number, to: number) => {
		if (overlapsTweak(from, to)) return;
		marks.push({ from, to });
		decos.push(hide.range(from, to));
	};

	const tree = ensureSyntaxTree(state, doc.length, 50) ?? syntaxTree(state);
	tree.iterate({
		enter: (ref) => {
			const heading = HEADING.exec(ref.name);
			if (heading) {
				decos.push(Decoration.line({ class: `cm-live-h${heading[1]}` }).range(lineStart(doc.lineAt(ref.from).from)));
				if (!revealed(ref.from, ref.to)) {
					const mark = ref.node.getChild("HeaderMark");
					if (mark) hideMark(mark.from, doc.sliceString(mark.to, mark.to + 1) === " " ? mark.to + 1 : mark.to);
				}
				return;
			}
			if (ref.name === "ListMark" && ref.node.parent?.name === "ListItem") {
				listItem(ref.node, doc, decos, revealed, overlapsTweak, lineStart);
				return;
			}
			const styleClass = INLINE_STYLE[ref.name];
			if (styleClass) decos.push(Decoration.mark({ class: styleClass }).range(ref.from, ref.to));
			const markName = INLINE_MARK[ref.name];
			if (markName && !revealed(ref.from, ref.to)) {
				for (const mark of ref.node.getChildren(markName)) hideMark(mark.from, mark.to);
			}
			if (ref.name === "Link") {
				const linkMarks = ref.node.getChildren("LinkMark");
				if (linkMarks.length < 2) return;
				decos.push(Decoration.mark({ class: "cm-live-link" }).range(linkMarks[0].to, linkMarks[1].from));
				if (!revealed(ref.from, ref.to)) {
					hideMark(linkMarks[0].from, linkMarks[0].to);
					hideMark(linkMarks[1].from, ref.to);
				}
			}
		},
	});

	return { decorations: Decoration.set(decos, true), marks, tweakRegions, tweakAtomic, spans, standalone };
}

/** Preview-style list item: bullet or number in the marker column, task marks as checkboxes. */
function listItem(
	mark: SyntaxNode,
	doc: EditorState["doc"],
	decos: CmRange<Decoration>[],
	revealed: (from: number, to: number) => boolean,
	overlapsTweak: (from: number, to: number) => boolean,
	lineStart: (pos: number) => number,
) {
	const line = doc.lineAt(mark.from);
	if (revealed(line.from, line.to)) return;
	// A marker after other text (blockquote, nested container) stays source.
	if (doc.sliceString(line.from, mark.from).trim() !== "") return;
	let depth = -1;
	for (let n: SyntaxNode | null = mark.node.parent; n; n = n.parent) if (n.name === "ListItem") depth++;
	const ordered = mark.node.parent?.parent?.name === "OrderedList";
	const contentFrom = doc.sliceString(mark.to, mark.to + 1) === " " ? mark.to + 1 : mark.to;
	if (overlapsTweak(line.from, contentFrom)) return;
	decos.push(
		Decoration.line({
			attributes: { style: `padding-left:${(depth + 1) * LIST_INDENT_EM}em;text-indent:-${LIST_INDENT_EM}em` },
		}).range(lineStart(line.from)),
	);
	if (ordered) {
		if (mark.from > line.from) decos.push(hide.range(line.from, mark.from));
		decos.push(Decoration.mark({ class: "cm-live-olmark" }).range(mark.from, contentFrom));
	} else {
		decos.push(
			Decoration.replace({ widget: new BulletWidget(BULLETS[depth % BULLETS.length]) }).range(line.from, contentFrom),
		);
	}
	const task = TASK_MARK.exec(doc.sliceString(contentFrom, Math.min(line.to, contentFrom + 4)));
	if (task && !overlapsTweak(contentFrom, contentFrom + 3)) {
		decos.push(
			Decoration.replace({ widget: new CheckboxWidget(task[1], contentFrom) }).range(contentFrom, contentFrom + 3),
		);
	}
}

const INLINE_STYLE: Record<string, string> = {
	Emphasis: "cm-live-em",
	StrongEmphasis: "cm-live-strong",
	Strikethrough: "cm-live-strike",
	InlineCode: "cm-live-code",
};
const INLINE_MARK: Record<string, string> = {
	Emphasis: "EmphasisMark",
	StrongEmphasis: "EmphasisMark",
	Strikethrough: "StrikethroughMark",
	InlineCode: "CodeMark",
};

const liveField = StateField.define<LiveValue>({
	create: buildLive,
	update(value, tr) {
		if (
			tr.docChanged ||
			tr.selection ||
			tr.effects.some((e) => e.is(setLiveFocus)) ||
			syntaxTree(tr.state) !== syntaxTree(tr.startState)
		)
			return buildLive(tr.state);
		return value;
	},
	provide: (f) => EditorView.decorations.from(f, (v) => v.decorations),
});

export function liveDecorations(state: EditorState): DecorationSet {
	return state.field(liveField).decorations;
}

/** Markdown syntax marks hidden right now (off the cursor lines). */
export function liveHiddenRanges(state: EditorState): Range[] {
	return state.field(liveField).marks;
}

/** Tweak comment syntax: hidden regardless of the cursor, and atomic. */
export function tweakHiddenRanges(state: EditorState): Range[] {
	return state.field(liveField).tweakRegions;
}

/**
 * Rewrite edits (positions in the old document) so no edit leaves half a tweak
 * comment behind.
 *  - An edit that covers all the highlighted text of a comment grows to remove
 *    the whole comment, markers and body included.
 *  - A bare delete of a hidden marker (Backspace/Delete beside a comment) deletes the
 *    adjacent highlighted character instead.
 *  - Any other edit is trimmed so it never cuts into marker syntax, which is
 *    hidden and cannot be edited by hand. Standalone comments (block, item,
 *    convention header) are removed only when an edit covers them entirely.
 */
export function protectTweakEdits(
	edits: readonly DocEdit[],
	spans: readonly TweakInlineSpan[],
	standalone: readonly TweakSyntaxRegion[],
): DocEdit[] {
	const out: DocEdit[] = [];
	for (const edit of edits) {
		let { from, to } = edit;
		if (from === to) {
			const inside = [...spans.flatMap((s) => [s.begin, s.end]), ...standalone].find(
				(r) => from > r.from && from < r.to,
			);
			out.push({ from: inside ? inside.from : from, to: inside ? inside.from : to, insert: edit.insert });
			continue;
		}
		// Backspace/Delete beside a comment targets its hidden marker; act on the nearest highlighted char instead.
		if (!edit.insert) {
			for (const s of spans) {
				if (s.highlight.to === s.highlight.from) continue;
				if (from === s.end.from && to === s.end.to) [from, to] = [s.highlight.to - 1, s.highlight.to];
				else if (from === s.begin.from && to === s.begin.to) [from, to] = [s.highlight.from, s.highlight.from + 1];
			}
		}
		for (let grown = true; grown; ) {
			grown = false;
			for (const s of spans) {
				const covers = s.highlight.to > s.highlight.from && from <= s.highlight.from && to >= s.highlight.to;
				if (covers && (from > s.begin.from || to < s.end.to)) {
					from = Math.min(from, s.begin.from);
					to = Math.max(to, s.end.to);
					grown = true;
				}
			}
		}
		const protectedRegions: TweakSyntaxRegion[] = [];
		for (const s of spans) {
			if (from <= s.begin.from && to >= s.end.to) continue;
			protectedRegions.push(s.begin, s.end);
		}
		for (const r of standalone) {
			if (!(from <= r.from && to >= r.to)) protectedRegions.push(r);
		}
		const pieces: Range[] = [];
		let cursor = from;
		for (const r of protectedRegions.filter((p) => p.from < to && from < p.to).sort((a, b) => a.from - b.from)) {
			if (r.from > cursor) pieces.push({ from: cursor, to: r.from });
			cursor = Math.max(cursor, r.to);
		}
		if (cursor < to) pieces.push({ from: cursor, to });
		if (pieces.length === 0) {
			if (edit.insert) out.push({ from: cursor, to: cursor, insert: edit.insert });
			continue;
		}
		pieces.forEach((p, i) => out.push({ from: p.from, to: p.to, insert: i === 0 ? edit.insert : "" }));
	}
	return out;
}

/** Marks a transaction the editor built itself (add comment); the guard leaves it alone. */
const trusted = Annotation.define<boolean>();

const tweakGuard = EditorState.transactionFilter.of((tr): TransactionSpec | readonly TransactionSpec[] => {
	if (!tr.docChanged || tr.annotation(trusted)) return tr;
	const event = tr.annotation(Transaction.userEvent);
	if (!event || event.startsWith("undo") || event.startsWith("redo")) return tr;
	const { spans, standalone } = tr.startState.field(liveField);
	if (spans.length === 0 && standalone.length === 0) return tr;
	const edits: DocEdit[] = [];
	tr.changes.iterChanges((from, to, _fb, _tb, inserted) => edits.push({ from, to, insert: inserted.toString() }));
	const safe = protectTweakEdits(edits, spans, standalone);
	const same = safe.length === edits.length && safe.every((e, i) => e.from === edits[i].from && e.to === edits[i].to);
	if (same) return tr;
	return { changes: safe, annotations: Transaction.userEvent.of(event), scrollIntoView: true };
});

/**
 * The preview's look, read from the same tokens as `markdown-content.css`
 * (a test compares the two). Headings are line decorations, so the rule under h1/h2 spans the line.
 */
const liveTheme = EditorView.theme({
	".cm-scroller": { fontFamily: "var(--font-ui)", fontSize: "var(--font-lg)" },
	".cm-content": { padding: "40px", lineHeight: "1.6", color: "var(--fg-primary)" },
	".cm-line": { padding: "0" },
	".cm-live-h1, .cm-live-h2, .cm-live-h3, .cm-live-h4, .cm-live-h5, .cm-live-h6": {
		fontWeight: "600",
		lineHeight: "1.25",
	},
	".cm-live-h1, .cm-live-h2": {
		borderBottomWidth: "1px",
		borderBottomStyle: "solid",
		borderBottomColor: "var(--border)",
		paddingBottom: "0.3em",
	},
	".cm-live-h1": { fontSize: "2em" },
	".cm-live-h2": { fontSize: "1.5em" },
	".cm-live-h3": { fontSize: "1.25em" },
	".cm-live-h4": { fontSize: "1em" },
	".cm-live-h5": { fontSize: "0.875em" },
	".cm-live-h6": { fontSize: "0.85em", color: "var(--fg-secondary)" },
	".cm-live-strong": { fontWeight: "700" },
	".cm-live-em": { fontStyle: "italic" },
	".cm-live-strike": { textDecoration: "line-through" },
	".cm-live-code": {
		fontFamily: "var(--font-mono)",
		fontSize: "85%",
		padding: "0.2em 0.4em",
		backgroundColor: "rgba(175, 184, 193, 0.2)",
		borderRadius: "var(--radius-lg)",
	},
	".cm-live-link": { color: "var(--accent)", textDecoration: "none" },
	".cm-live-bullet, .cm-live-olmark": { display: "inline-block", minWidth: "2em", textIndent: "0" },
	".cm-live-bullet": { textAlign: "right", boxSizing: "border-box", paddingRight: "0.4em" },
	".cm-live-bullet-nested": { fontSize: "0.7em" },
	".cm-live-checkbox": { margin: "0 0.5em 0 0", verticalAlign: "middle", cursor: "pointer" },
	".tweak-highlight": {
		background: "color-mix(in srgb, var(--tweak-highlight) 25%, transparent)",
		borderBottom: "1.5px solid color-mix(in srgb, var(--tweak-highlight) 70%, transparent)",
	},
	".tweak-highlight:hover": { background: "color-mix(in srgb, var(--tweak-highlight) 40%, transparent)" },
});

/** Live-preview extension: hidden marks, tweak presentation, atomic markers, edit guard. */
export function liveMarkdown(): Extension {
	return [
		focusField,
		liveField,
		EditorView.atomicRanges.of((view) => view.state.field(liveField).tweakAtomic),
		tweakGuard,
		EditorView.focusChangeEffect.of((_state, focused) => setLiveFocus.of(focused)),
		Prec.high(liveTheme),
	];
}

/**
 * Wrap the main selection in a tweak comment with the viewer's own writer
 * (`insertTweakComment`), so the file format has a single producer. Throws
 * `OverlappingCommentError` when the selection touches an existing comment.
 */
export function addTweakCommentAtSelection(view: EditorView, comment: Omit<TweakComment, "highlighted">): void {
	const { from, to } = view.state.selection.main;
	if (from === to) return;
	const source = view.state.sliceDoc();
	const highlighted = view.state.sliceDoc(from, to);
	// `source` spells a line break with two characters in a CRLF file, the document with one.
	const sourceFrom = view.state.lineBreak === "\r\n" ? from + view.state.doc.lineAt(from).number - 1 : from;
	// `insertTweakComment` counts occurrences the way the viewer's DOM does; find the one at the selection.
	let occurrence = 0;
	for (let n = 0; ; n++) {
		const match = findSourceMatch(source, highlighted, n);
		if (!match) break;
		occurrence = n;
		if (match.start >= sourceFrom) break;
	}
	const updated = insertTweakComment(source, { ...comment, highlighted }, occurrence);
	// Diff in CodeMirror's coordinates (one character per line break, whatever the file uses).
	const crlf = view.state.lineBreak === "\r\n";
	const before = view.state.doc.toString();
	const after = updated.replace(/\r\n/g, "\n");
	let start = 0;
	while (start < before.length && before[start] === after[start]) start++;
	let end = 0;
	while (end < before.length - start && before[before.length - 1 - end] === after[after.length - 1 - end]) end++;
	const insert = after.slice(start, after.length - end);
	view.dispatch({
		changes: { from: start, to: before.length - end, insert: crlf ? insert.replace(/\n/g, "\r\n") : insert },
		annotations: [trusted.of(true), Transaction.userEvent.of("input.tweak")],
	});
}
