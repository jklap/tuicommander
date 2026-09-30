// @vitest-environment jsdom

import { readFileSync } from "node:fs";
import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LiveMarkdownEditor } from "../../components/MarkdownTab/LiveMarkdownEditor";
import { CONVENTION_HEADER } from "../../utils/tweakComments";

// jsdom has no layout; CodeMirror's measure pass asks Range for client rects.
Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

afterEach(cleanup);

/** The preview's stylesheet is the oracle: Live must read the same values, not restate them. */
const PREVIEW_CSS = readFileSync("src/components/ui/markdown-content.css", "utf8");
function previewDecl(selector: string, prop: string): string {
	const esc = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	const block = new RegExp(`(?:^|\\n|,\\s*)${esc}\\s*\\{([^}]*)\\}`).exec(PREVIEW_CSS);
	const decl = block && new RegExp(`(?:^|;|\\s)${prop}:\\s*([^;]+);`).exec(block[1]);
	if (!decl) throw new Error(`preview CSS has no ${prop} for ${selector}`);
	return decl[1].trim();
}

/** jsdom reports no document focus, which CodeMirror requires; pretend the window is active. */
function focus(view: EditorView) {
	vi.spyOn(document, "hasFocus").mockReturnValue(true);
	view.focus();
}

function mount(doc: string) {
	const { container } = render(() => (
		<LiveMarkdownEditor content={doc} onSave={vi.fn().mockResolvedValue(true)} readDisk={async () => doc} />
	));
	const root = container.querySelector(".cm-editor") as HTMLElement;
	const view = EditorView.findFromDOM(root) as EditorView;
	const lines = () => Array.from(root.querySelectorAll<HTMLElement>(".cm-line"));
	return { view, root, lines };
}
/** jsdom resolves em against its 16px default; the preview writes em. */
const emToPx = (v: string, base = 16) =>
	v
		.split(" ")
		.map((p) =>
			/(em|%)$/.test(p) ? `${Number((parseFloat(p) * (p.endsWith("%") ? base / 100 : base)).toFixed(2))}px` : p,
		)
		.join(" ");
const css = (el: Element) => getComputedStyle(el);

describe("Live typography comes from the preview stylesheet", () => {
	const doc = "# Title\n\n## Sub\n\nprose with `code` here\n";

	it("uses the preview font, size and line height for the text area", () => {
		// catches: Live inherits the code editor monospace font and sizes instead of the preview prose styles
		const t = mount(doc);
		const scroller = t.root.querySelector(".cm-scroller") as HTMLElement;
		const content = t.root.querySelector(".cm-content") as HTMLElement;
		expect(css(scroller).fontFamily).toBe(previewDecl("#markdown-content", "font-family"));
		expect(css(scroller).fontSize).toBe(previewDecl("#markdown-content", "font-size"));
		expect(css(content).lineHeight).toBe(previewDecl("#markdown-content", "line-height"));
	});

	it("sizes headings like the preview and draws the h1/h2 rule", () => {
		// catches: heading sizes/weights/rules differ from the preview
		const t = mount(doc);
		const [h1, , h2] = t.lines();
		expect(css(h1).fontSize).toBe(emToPx(previewDecl("#markdown-content h1", "font-size")));
		expect(css(h2).fontSize).toBe(emToPx(previewDecl("#markdown-content h2", "font-size")));
		// font-weight sits in the h1..h6 group rule, whose last selector is h6
		expect(css(h1).fontWeight).toBe(previewDecl("#markdown-content h6", "font-weight"));
		expect(css(h1).borderBottomStyle).toBe("solid");
		expect(css(h1).borderBottomWidth).toBe("1px");
	});

	it("draws inline code as the preview chip in the mono font", () => {
		// catches: inline code without the preview chip (padding, background, size)
		const t = mount(doc);
		const chip = t.root.querySelector(".cm-live-code") as HTMLElement;
		expect(css(chip).fontFamily).toBe(previewDecl("#markdown-content code", "font-family"));
		expect(css(chip).padding).toBe(emToPx(previewDecl("#markdown-content code", "padding"), 16 * 0.85));
		expect(css(chip).fontSize).toBe(emToPx(previewDecl("#markdown-content code", "font-size")));
		expect(css(chip).backgroundColor.replace(/\s/g, "")).toBe(
			previewDecl("#markdown-content code", "background-color").replace(/\s/g, ""),
		);
	});
});

describe("Live lists", () => {
	const doc = "- one\n  - nested\n1. first\n\nlast line\n";

	it("shows bullets instead of raw markers off the cursor line", () => {
		// catches: -, * and 1. list markers shown raw instead of bullets/numbers with the preview indentation
		const t = mount(doc);
		t.view.dispatch({ selection: { anchor: doc.length - 1 } });
		const [one, nested, first] = t.lines();
		expect(one.textContent).not.toContain("- ");
		expect(one.querySelector(".cm-live-bullet")).not.toBeNull();
		expect(nested.querySelector(".cm-live-bullet")).not.toBeNull();
		expect(nested.textContent).toContain("nested");
		expect(nested.textContent).not.toContain("-");
		// ordered lists keep their number, without the raw indentation
		expect(first.textContent).toBe("1. first");
		// nesting indents by the preview's list padding (2em per level)
		const step = parseFloat(emToPx(previewDecl("#markdown-content ol", "padding-left")));
		expect(parseFloat(css(one).paddingLeft)).toBe(step);
		expect(parseFloat(css(nested).paddingLeft)).toBe(2 * step);
	});

	it("shows the raw marker on the cursor line only", async () => {
		// catches: the cursor line stays rendered so the source cannot be edited
		const t = mount(doc);
		focus(t.view);
		t.view.dispatch({ selection: { anchor: 2 } });
		const [one, nested] = t.lines();
		await waitFor(() => expect(one.textContent).toBe("- one"));
		expect(nested.querySelector(".cm-live-bullet")).not.toBeNull();
	});
});

describe("Live task checkboxes", () => {
	const doc = "- [ ] todo\n- [x] done\n- [~] doing\n\nend\n";
	const boxes = (root: HTMLElement) => Array.from(root.querySelectorAll<HTMLInputElement>("input.cm-live-checkbox"));

	it("renders [ ], [x] and [~] as checkboxes off the cursor line", () => {
		// catches: [ ], [x], [~] shown as raw text
		const t = mount(doc);
		t.view.dispatch({ selection: { anchor: doc.length - 1 } });
		const b = boxes(t.root);
		expect(b).toHaveLength(3);
		expect(b.map((x) => x.checked)).toEqual([false, true, false]);
		expect(b[2].indeterminate).toBe(true);
		expect(t.lines()[0].textContent).not.toContain("[");
	});

	it("clicking rewrites only that bracket character", () => {
		// catches: the click rewrites more than the mark (lossless save) or the wrong line
		const t = mount(doc);
		t.view.dispatch({ selection: { anchor: doc.length - 1 } });
		fireEvent.click(boxes(t.root)[0]);
		expect(t.view.state.sliceDoc()).toBe("- [x] todo\n- [x] done\n- [~] doing\n\nend\n");
		fireEvent.click(boxes(t.root)[1]);
		expect(t.view.state.sliceDoc()).toBe("- [x] todo\n- [~] done\n- [~] doing\n\nend\n");
		fireEvent.click(boxes(t.root)[2]);
		expect(t.view.state.sliceDoc()).toBe("- [x] todo\n- [~] done\n- [ ] doing\n\nend\n");
	});

	it("leaves a bracket pair in prose alone", () => {
		// catches: any [x] in running text becoming a checkbox
		const t = mount("say [x] now\n\nend\n");
		t.view.dispatch({ selection: { anchor: 15 } });
		expect(boxes(t.root)).toHaveLength(0);
	});
});

describe("Live headings", () => {
	it("hides the heading mark on the first line when the editor opens unfocused", () => {
		// catches: the first heading keeps its # when the cursor is on another line or the editor is unfocused on open
		const t = mount("# Title\n\ntext\n");
		expect(t.lines()[0].textContent).toBe("Title");
	});

	it("styles a heading that follows the hidden tweak convention header", () => {
		// catches: the heading line decoration lost inside the hidden header, so the first heading looks like prose
		const t = mount(`${CONVENTION_HEADER}# Title\n\ntext\n`);
		expect(t.lines()[0].className).toContain("cm-live-h1");
	});

	it("shows the mark while the cursor is on the heading and the editor has focus", async () => {
		// catches: the cursor line can never be edited as source
		const t = mount("# Title\n\ntext\n");
		focus(t.view);
		t.view.dispatch({ selection: { anchor: 3 } });
		await waitFor(() => expect(t.lines()[0].textContent).toBe("# Title"));
		t.view.dispatch({ selection: { anchor: 12 } });
		await waitFor(() => expect(t.lines()[0].textContent).toBe("Title"));
	});
});
