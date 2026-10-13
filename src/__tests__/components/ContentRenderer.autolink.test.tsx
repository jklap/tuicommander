// @vitest-environment jsdom

import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { ContentRenderer } from "../../components/ui/ContentRenderer";

const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

async function renderLinked(content: string, onLinkClick = vi.fn()) {
	const view = render(() => <ContentRenderer content={content} autoLinkFiles onLinkClick={onLinkClick} />);
	await settle();
	return { ...view, onLinkClick };
}

describe("ContentRenderer file auto-link", () => {
	// Catches: the walker skipping every <code>, so a backticked path is never clickable.
	it("links inline code that holds only a file path, with and without a line", async () => {
		const { container, onLinkClick } = await renderLinked("See `src/a.ts` and `src/b.ts:12` now.");
		const anchors = [...container.querySelectorAll<HTMLAnchorElement>("a[data-tuic-href]")];
		expect(anchors.map((a) => a.dataset.tuicHref)).toEqual(["src/a.ts", "src/b.ts:12"]);
		expect(anchors.every((a) => a.closest("code"))).toBe(true);
		fireEvent.click(anchors[1]);
		expect(onLinkClick).toHaveBeenCalledWith("src/b.ts:12");
	});

	// Catches: linking every inline code that merely contains a path-like token.
	it("leaves inline code alone when the path is only part of the text", async () => {
		const { container } = await renderLinked("Run `cat src/a.ts` or `npm run build` or `foo`.");
		expect(container.querySelector("a")).toBeNull();
	});

	// Catches: the fix leaking into fenced blocks.
	it("keeps fenced code blocks unlinked", async () => {
		const { container } = await renderLinked("```\nsrc/a.ts\n```\n");
		expect(container.querySelector("pre")?.textContent).toContain("src/a.ts");
		expect(container.querySelector("a")).toBeNull();
	});

	// Catches: inline code already inside a markdown link getting a nested anchor.
	it("does not nest an anchor inside an existing link", async () => {
		const { container } = await renderLinked("[`src/a.ts`](src/a.ts)");
		expect(container.querySelectorAll("a").length).toBe(1);
	});
});
