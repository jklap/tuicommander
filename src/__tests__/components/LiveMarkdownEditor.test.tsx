// @vitest-environment jsdom

import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LiveMarkdownEditor } from "../../components/MarkdownTab/LiveMarkdownEditor";

// jsdom has no layout; CodeMirror's measure pass asks Range for client rects.
Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

afterEach(cleanup);

function setup(initial: string, disk: () => string) {
	const [content, setContent] = createSignal(initial);
	const onSave = vi.fn().mockResolvedValue(true);
	const readDisk = vi.fn(async () => disk());
	const { container } = render(() => <LiveMarkdownEditor content={content()} onSave={onSave} readDisk={readDisk} />);
	const view = () => EditorView.findFromDOM(container.querySelector(".cm-editor") as HTMLElement) as EditorView;
	const type = (text: string) => view().dispatch({ changes: { from: 0, insert: text }, userEvent: "input.type" });
	/** Click the first rendered block, as a user would, to swap it for its source editor. */
	const open = async () => {
		const block = await waitFor(() => {
			const el = container.querySelector<HTMLElement>("[data-comment-source-start]");
			expect(el).not.toBeNull();
			return el as HTMLElement;
		});
		fireEvent.click(block);
		await waitFor(() => expect(container.querySelector(".cm-editor")).not.toBeNull());
	};
	return { content, setContent, onSave, readDisk, view, type, open, container };
}

describe("LiveMarkdownEditor disk changes", () => {
	it("reloads a clean buffer when the file changes on disk", async () => {
		// catches: stale text shown after an agent rewrote the file
		const t = setup("old", () => "old");
		t.setContent("new from agent");
		await waitFor(() => expect(t.container.textContent).toContain("new from agent"));
	});

	it("keeps a dirty buffer when the file changes on disk", async () => {
		// catches: local typing thrown away by a reload
		const t = setup("old", () => "old");
		await t.open();
		t.type("mine ");
		t.setContent("agent text");
		await new Promise((r) => setTimeout(r, 20));
		expect(t.view().state.sliceDoc()).toBe("mine old");
	});

	it("refuses to save over a concurrent external edit and offers Reload and Overwrite", async () => {
		// catches: live save overwrites a concurrent external edit
		let disk = "old";
		const t = setup("old", () => disk);
		await t.open();
		t.type("mine ");
		disk = "old + agent edit";
		fireEvent.click(await screen.findByText("Save"));
		await screen.findByText("Overwrite");
		expect(screen.getByText("Reload")).not.toBeNull();
		expect(t.onSave).not.toHaveBeenCalled();
	});

	it("Overwrite writes the buffer", async () => {
		// catches: no way past the banner
		let disk = "old";
		const t = setup("old", () => disk);
		await t.open();
		t.type("mine ");
		disk = "changed";
		fireEvent.click(await screen.findByText("Save"));
		fireEvent.click(await screen.findByText("Overwrite"));
		await waitFor(() => expect(t.onSave).toHaveBeenCalledWith("mine old"));
	});

	it("Reload replaces the buffer with the disk text and does not revert to stale content", async () => {
		// catches: Reload discarded, or the stale content prop re-applied afterwards
		let disk = "old";
		const t = setup("old", () => disk);
		await t.open();
		t.type("mine ");
		disk = "changed";
		fireEvent.click(await screen.findByText("Save"));
		fireEvent.click(await screen.findByText("Reload"));
		await waitFor(() => expect(t.container.textContent).toContain("changed"));
		expect(t.container.querySelector(".cm-editor")).toBeNull();
		expect(t.onSave).not.toHaveBeenCalled();
		expect(screen.queryByText("Overwrite")).toBeNull();
	});

	it("saves normally when the disk still holds what was loaded", async () => {
		// catches: guard blocking every save
		const t = setup("old", () => "old");
		await t.open();
		t.type("mine ");
		fireEvent.click(await screen.findByText("Save"));
		await waitFor(() => expect(t.onSave).toHaveBeenCalledWith("mine old"));
	});
});
