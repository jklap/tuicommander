// @vitest-environment jsdom
import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HtmlPreviewTab } from "../../components/HtmlPreviewTab/HtmlPreviewTab";
import { ContentRenderer } from "../../components/ui/ContentRenderer";
import type { HtmlPreviewTab as PreviewTab } from "../../stores/mdTabs";
import { repositoriesStore } from "../../stores/repositories";
import { rewriteLocalImages } from "../../utils/repoImageUrl";

vi.mock("../../transport", async (original) => ({ ...(await original<object>()), isTauri: () => false }));
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue('<html><head></head><body><img src="shot.png"></body></html>'),
	listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({ readFile: vi.fn().mockResolvedValue('<img src="shot.png">') }),
}));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn(), openLocalPath: vi.fn() }));

beforeEach(() => repositoriesStore.add({ path: "/repo", displayName: "Repo" }));
afterEach(() => {
	cleanup();
	repositoriesStore._testCancelPendingSave();
});
function tab(filePath: string): PreviewTab {
	return {
		id: "preview",
		title: "Preview",
		type: "html-preview",
		repoPath: "",
		filePath,
		fileName: filePath.split("/").pop() ?? "",
	};
}
function imageUrl(container: HTMLElement): URL {
	const src = container.querySelector("img")?.getAttribute("src");
	if (!src) throw new Error("Expected a repository image");
	return new URL(src);
}
describe("browser repository preview boundary", () => {
	it.each(["![relative](shot.png)", "![absolute](/outside/shot.png)"])(
		"replaces an external Markdown image with a placeholder — catches: bare local paths producing broken images (%s)",
		(content) => {
			const { container, getByRole } = render(() => <ContentRenderer content={content} baseDir="/outside" />);
			expect(container.querySelector("img")).toBeNull();
			expect(getByRole("img", { name: "Image unavailable" }).textContent).toContain("outside open repositories");
		},
	);
	it.each(["![relative](shot.png)", "![absolute](/repo/docs/shot.png)"])(
		"routes an in-repository Markdown image — catches: absolute paths or baseDir bypassing the HTTP image route (%s)",
		(content) => {
			const { container } = render(() => <ContentRenderer content={content} baseDir="/repo/docs" />);
			const url = imageUrl(container);
			expect(url.pathname).toBe("/fs/markdown-image");
			expect(url.searchParams.get("repoPath")).toBe("/repo");
			expect(url.searchParams.get("file")).toBe("docs/shot.png");
		},
	);
	it.each(["<img src='shot.png'>", "<img src=shot.png>"])(
		"resolves HTML image attribute forms — catches: only double-quoted sources being mapped (%s)",
		(html) => {
			const doc = new DOMParser().parseFromString(rewriteLocalImages(html, "/repo/docs"), "text/html");
			expect(new URL(doc.querySelector("img")?.getAttribute("src") ?? "").searchParams.get("file")).toBe(
				"docs/shot.png",
			);
		},
	);
	it("keeps a mobile repository resolver — catches: browser fallback overriding the caller's image route", () => {
		const { container } = render(() => (
			<ContentRenderer
				content="![x](shot.png)"
				imageSrc={() => "https://phone.example/fs/markdown-image?file=docs/shot.png"}
			/>
		));
		expect(imageUrl(container).host).toBe("phone.example");
	});
	it("replaces an external mobile absolute path — catches: mobile imageSrc returning an unservable local path", () => {
		const { container, getByRole } = render(() => (
			<ContentRenderer content="![x](/outside/shot.png)" imageSrc={(path) => path} />
		));
		expect(container.querySelector("img")).toBeNull();
		expect(getByRole("img", { name: "Image unavailable" })).toBeDefined();
	});
	it("routes an absolute repository image tab — catches: absolute tab paths falling through to convertFileSrc", () => {
		const { container } = render(() => <HtmlPreviewTab tab={tab("/repo/docs/shot.png")} />);
		expect(imageUrl(container).searchParams.get("file")).toBe("docs/shot.png");
	});
	it("shows a placeholder for an external image tab — catches: external tabs rendering a broken img", () => {
		const { container, getByRole } = render(() => <HtmlPreviewTab tab={tab("/outside/shot.png")} />);
		expect(container.querySelector("img")).toBeNull();
		expect(getByRole("img", { name: "Image unavailable" })).toBeDefined();
	});
	it("blocks an external HTML tab — catches: HTML preview reading a file outside the repository boundary", async () => {
		const { container } = render(() => <HtmlPreviewTab tab={tab("/outside/page.html")} />);
		await waitFor(() => expect(container.textContent).toContain("Preview unavailable: outside open repositories."));
		expect(container.querySelector("iframe")).toBeNull();
	});
	it("routes local images in an HTML preview — catches: HTML base tags pointing browser images at bare paths", async () => {
		const { container } = render(() => <HtmlPreviewTab tab={tab("/repo/docs/page.html")} />);
		await waitFor(() =>
			expect(container.querySelector("iframe")?.getAttribute("srcdoc")).toContain("/fs/markdown-image"),
		);
		expect(container.querySelector("iframe")?.getAttribute("srcdoc")).toContain("docs%2Fshot.png");
	});
});
