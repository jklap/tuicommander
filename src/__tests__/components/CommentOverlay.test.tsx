// @vitest-environment jsdom

import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommentOverlay } from "../../components/MarkdownTab/CommentOverlay";

afterEach(() => cleanup());

describe("CommentOverlay block gutter", () => {
	it("does not open the block comment when an interactive checkbox inside it is clicked", () => {
		const scrollHost = document.createElement("div");
		const content = document.createElement("div");
		const list = document.createElement("ul");
		list.className = "tweak-block-highlight";
		list.dataset.tweakId = "c_tasks";
		list.dataset.tweakComment = "Review these tasks";
		list.dataset.tweakAt = "2026-09-23T08:00:00.000Z";
		const checkbox = document.createElement("input");
		checkbox.type = "checkbox";
		list.append(checkbox);
		content.append(list);
		scrollHost.append(content);
		document.body.append(scrollHost);

		render(() => <CommentOverlay contentRef={content} onSave={vi.fn()} onDelete={vi.fn()} />);
		fireEvent.click(checkbox);

		expect(document.body.querySelector("textarea")).toBeNull();
	});

	it("opens a comment for the exact source range when the gutter icon is clicked", () => {
		const scrollHost = document.createElement("div");
		const content = document.createElement("div");
		const paragraph = document.createElement("p");
		paragraph.textContent = "Rendered formatted paragraph.";
		paragraph.dataset.commentSourceStart = "18";
		paragraph.dataset.commentSourceEnd = "61";
		content.append(paragraph);
		scrollHost.append(content);
		document.body.append(scrollHost);
		paragraph.getBoundingClientRect = () =>
			({ left: 100, right: 500, top: 80, bottom: 120, width: 400, height: 40, x: 100, y: 80, toJSON() {} }) as DOMRect;
		scrollHost.getBoundingClientRect = () =>
			({ left: 60, right: 600, top: 20, bottom: 500, width: 540, height: 480, x: 60, y: 20, toJSON() {} }) as DOMRect;

		const onSaveBlock = vi.fn();
		render(() => <CommentOverlay contentRef={content} onSave={vi.fn()} onSaveBlock={onSaveBlock} onDelete={vi.fn()} />);

		fireEvent.mouseMove(scrollHost, { clientX: 76, clientY: 96 });
		const button = document.body.querySelector('[aria-label="Comment on this block"]') as HTMLButtonElement;
		expect(button).not.toBeNull();
		fireEvent.mouseDown(button);
		fireEvent.input(document.body.querySelector("textarea")!, { target: { value: "Clarify this block" } });
		fireEvent.click(Array.from(document.body.querySelectorAll("button")).find((el) => el.textContent === "Save")!);

		expect(onSaveBlock).toHaveBeenCalledWith(
			expect.objectContaining({ comment: "Clarify this block", highlighted: "Rendered formatted paragraph." }),
			{ start: 18, end: 61 },
		);
	});
});
