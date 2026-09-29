import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { ensureSyntaxTree, type LanguageSupport, syntaxTree } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import {
	Annotation,
	EditorState,
	type Extension,
	type Range as CmRange,
	StateField,
	Transaction,
	type TransactionSpec,
} from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";
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

/**
 * True when Live mode can save the file byte for byte. CodeMirror keeps one line
 * separator per document, so mixed endings (or a lone CR) would be rewritten.
 */
export function liveModeSupported(text: string): boolean {
	if (text.length > LIVE_MAX_CHARS) return false;
	if (/\r(?!\n)/.test(text)) return false;
	return !(text.includes("\r\n") && /(?<!\r)\n/.test(text));
}

/** Pin the separator to the file's own so `state.sliceDoc()` reproduces it. Also stops CM
 *  from splitting on U+2028/U+2029, which it would otherwise turn into newlines. */
export function liveLineSeparator(text: string): Extension {
	return EditorState.lineSeparator.of(text.includes("\r\n") ? "\r\n" : "\n");
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
	const tweakRegions: Range[] = [
		...spans.flatMap((s) => [s.begin, s.end]),
		...standalone,
	].sort((a, b) => a.from - b.from);
	const tweakAtomic = Decoration.set(tweakRegions.map((r) => hide.range(r.from, r.to)));

	const decos: CmRange<Decoration>[] = tweakRegions.map((r) => hide.range(r.from, r.to));
	for (const s of spans) {
		if (s.highlight.to > s.highlight.from) {
			decos.push(
				Decoration.mark({ class: "tweak-highlight", attributes: { title: s.comment } }).range(s.highlight.from, s.highlight.to),
			);
		}
	}

	const marks: Range[] = [];
	const selected = selectedLineSpans(state);
	const revealed = (from: number, to: number) => {
		const a = doc.lineAt(from).number;
		const b = doc.lineAt(to).number;
		return selected.some((l) => l.from <= b && a <= l.to);
	};
	const overlapsTweak = (from: number, to: number) => tweakRegions.some((r) => from < r.to && r.from < to);
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
				decos.push(Decoration.line({ class: `cm-live-h${heading[1]}` }).range(doc.lineAt(ref.from).from));
				if (!revealed(ref.from, ref.to)) {
					const mark = ref.node.getChild("HeaderMark");
					if (mark) hideMark(mark.from, doc.sliceString(mark.to, mark.to + 1) === " " ? mark.to + 1 : mark.to);
				}
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
		if (tr.docChanged || tr.selection || syntaxTree(tr.state) !== syntaxTree(tr.startState)) return buildLive(tr.state);
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
			const inside = [...spans.flatMap((s) => [s.begin, s.end]), ...standalone].find((r) => from > r.from && from < r.to);
			out.push({ from: inside ? inside.from : from, to: inside ? inside.from : to, insert: edit.insert });
			continue;
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

const liveTheme = EditorView.baseTheme({
	".cm-live-h1": { fontSize: "1.6em", fontWeight: "700" },
	".cm-live-h2": { fontSize: "1.4em", fontWeight: "700" },
	".cm-live-h3": { fontSize: "1.2em", fontWeight: "700" },
	".cm-live-h4, .cm-live-h5, .cm-live-h6": { fontWeight: "700" },
	".cm-live-strong": { fontWeight: "700" },
	".cm-live-em": { fontStyle: "italic" },
	".cm-live-strike": { textDecoration: "line-through" },
	".cm-live-code": { fontFamily: "var(--font-mono)", background: "var(--bg-tertiary)", borderRadius: "3px" },
	".cm-live-link": { color: "var(--accent)", textDecoration: "underline" },
	".tweak-highlight": {
		background: "color-mix(in srgb, var(--tweak-highlight) 25%, transparent)",
		borderBottom: "1.5px solid color-mix(in srgb, var(--tweak-highlight) 70%, transparent)",
	},
	".tweak-highlight:hover": { background: "color-mix(in srgb, var(--tweak-highlight) 40%, transparent)" },
});

/** Live-preview extension: hidden marks, tweak presentation, atomic markers, edit guard. */
export function liveMarkdown(): Extension {
	return [
		liveField,
		EditorView.atomicRanges.of((view) => view.state.field(liveField).tweakAtomic),
		tweakGuard,
		liveTheme,
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
	const sourceFrom = source.includes("\r\n") ? from + view.state.doc.lineAt(from).number - 1 : from;
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
	const crlf = source.includes("\r\n");
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
