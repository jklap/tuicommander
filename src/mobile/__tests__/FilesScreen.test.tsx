// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FilesScreen } from "../screens/FilesScreen";

vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

const files = new Map([
	["src/hello.txt", "hello\n"],
	["src/guide.md", "# Guide\n\n**Important** note.\n"],
	["src/null.dat", "abc\0def"],
	["src/abs.md", "![In](/repo-one/src/images/in.png) ![Out](/elsewhere/out.png)"],
	[
		"src/deep/guide.md",
		"![Local](images/diagram.png) ![Web](https://example.com/logo.png) ![Inline](data:image/png;base64,AAAA)",
	],
]);
const calls: string[] = [];
const linkedTarget = vi.hoisted(() => ({
	value: { absolute_path: "/repo-one/src/deep/guide.md", is_directory: false },
}));
let failSave = false;
let delayedSearch: Promise<Array<{ name: string; path: string; is_dir: boolean; size: number }>> | null = null;

vi.mock("../../transport", () => ({
	rpc: vi.fn(async (command: string, args?: Record<string, string>) => {
		calls.push(command);
		if (command === "load_repositories") return { repos: { "/repo-one": {}, "/repo-two": {} } };
		if (command === "list_directory") {
			if (args?.subdir === "src")
				return [
					{ name: "hello.txt", path: "src/hello.txt", is_dir: false, size: 6 },
					{ name: "guide.md", path: "src/guide.md", is_dir: false, size: 29 },
					{ name: "huge.txt", path: "src/huge.txt", is_dir: false, size: 1_048_577 },
					{ name: "image.bin", path: "src/image.bin", is_dir: false, size: 32 },
					{ name: "null.dat", path: "src/null.dat", is_dir: false, size: 7 },
				];
			return [
				{ name: ".claude", path: ".claude", is_dir: true, size: 0 },
				{ name: "src", path: "src", is_dir: true, size: 0 },
				{ name: ".mdkb", path: ".mdkb", is_dir: true, size: 0 },
				{ name: "docs", path: "docs", is_dir: true, size: 0 },
			];
		}
		if (command === "search_files") {
			if (delayedSearch) return delayedSearch;
			return args?.query === "config"
				? [{ name: "config.ts", path: "src/deep/config.ts", is_dir: false, size: 12 }]
				: [];
		}
		if (command === "stat_path") return { exists: true, is_dir: false, size: 110 };
		if (command === "resolve_terminal_path") return linkedTarget.value;
		if (command === "fs_read_file") {
			if (args?.file === "src/image.bin") throw new Error("Failed to read file: stream did not contain valid UTF-8");
			return files.get(args?.file ?? "") ?? "";
		}
		if (command === "write_file") {
			if (failSave) throw new Error("disk full");
			files.set(args?.file ?? "", args?.content ?? "");
			return undefined;
		}
		throw new Error(`Unexpected command: ${command}`);
	}),
}));

const openRepo = async (getByRole: ReturnType<typeof render>["getByRole"]) => {
	await fireEvent.click(getByRole("button", { name: /repo-one/ }));
	await fireEvent.click(await waitFor(() => getByRole("button", { name: /src/ })));
};

afterEach(() => {
	cleanup();
	files.set("src/hello.txt", "hello\n");
	files.set("src/guide.md", "# Guide\n\n**Important** note.\n");
	calls.length = 0;
	failSave = false;
	delayedSearch = null;
	linkedTarget.value = { absolute_path: "/repo-one/src/deep/guide.md", is_directory: false };
});

describe("FilesScreen", () => {
	it("opens a backend-resolved chat directory without treating it as a file", async () => {
		linkedTarget.value = { absolute_path: "/home/boss/Gits/project/docs", is_directory: true };
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/home/boss/Gits", worktreePath: "/home/boss/Gits" }}
				initialLink={{ candidate: "project/docs/" }}
			/>
		));
		await waitFor(() => expect(view.getByRole("searchbox", { name: "Search files" })).toBeTruthy());
		expect(calls).toContain("resolve_terminal_path");
		expect(calls).toContain("list_directory");
		expect(calls).not.toContain("stat_path");
	});
	it("refuses a resolved directory outside the allowed workspace", async () => {
		linkedTarget.value = { absolute_path: "/secret/private", is_directory: true };
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/repo-one", worktreePath: "/repo-one" }}
				initialLink={{ candidate: "/secret/private" }}
			/>
		));
		await waitFor(() => expect(view.getByRole("alert").textContent).toMatch(/outside.*registered repository/i));
		expect(calls).not.toContain("list_directory");
	});

	it("shows the complete repository path on a long press", async () => {
		const view = render(() => <FilesScreen />);
		const repository = await waitFor(() => view.getByRole("button", { name: /repo-one/ }));
		await fireEvent.touchStart(repository);
		await waitFor(
			() => expect(view.getByRole("dialog", { name: "Repository path" }).textContent).toContain("/repo-one"),
			{
				timeout: 900,
			},
		);
		await fireEvent.touchEnd(repository);
		await fireEvent.click(repository);
		expect(calls).not.toContain("list_directory");
	});

	it("keeps file actions in the title bar in both viewing and editing", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /hello.txt/ }));
		await waitFor(() => expect(view.getByRole("button", { name: "Edit" })).toBeTruthy());
		expect(view.getByRole("button", { name: "Edit" }).closest("header")).toBe(
			view.getByRole("button", { name: "Back" }).closest("header"),
		);
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		expect(view.getByRole("button", { name: "Cancel" }).closest("header")).toBeTruthy();
		expect(view.getByRole("button", { name: "Save" }).closest("header")).toBeTruthy();
	});
	it("places normal folders before hidden folders in the tree", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: /repo-one/ }));
		await waitFor(() => expect(view.getByRole("button", { name: /docs/ })).toBeTruthy());
		const order = Array.from(view.container.querySelectorAll("button"))
			.map((button) => button.textContent?.trim())
			.filter(Boolean);
		expect(order.indexOf("▸ src")).toBeLessThan(order.indexOf("▸ .claude"));
		expect(order.indexOf("▸ docs")).toBeLessThan(order.indexOf("▸ .mdkb"));
	});

	it("finds a file in a nested folder from the repository tree", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: /repo-one/ }));
		await fireEvent.input(view.getByRole("searchbox", { name: "Search files" }), {
			target: { value: "config" },
		});
		await waitFor(() => expect(view.getByRole("button", { name: /src\/deep\/config.ts/ })).toBeTruthy());
		expect(view.queryByRole("button", { name: /\.claude/ })).toBeNull();
	});

	it("restores the tree when a cleared search returns late", async () => {
		let finishSearch!: (results: Array<{ name: string; path: string; is_dir: boolean; size: number }>) => void;
		delayedSearch = new Promise((resolve) => {
			finishSearch = resolve;
		});
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: /repo-one/ }));
		const search = view.getByRole("searchbox", { name: "Search files" });
		await fireEvent.input(search, { target: { value: "config" } });
		await fireEvent.input(search, { target: { value: "" } });
		finishSearch([{ name: "config.ts", path: "src/deep/config.ts", is_dir: false, size: 12 }]);
		await waitFor(() => expect(view.getByRole("button", { name: /src/ })).toBeTruthy());
		expect(view.queryByRole("button", { name: /src\/deep\/config.ts/ })).toBeNull();
	});
	it("lists configured repositories and navigates into and out of a directory", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		expect(view.getByRole("button", { name: /hello.txt/ })).toBeTruthy();
		await fireEvent.click(view.getByRole("button", { name: /back/i }));
		expect(view.getByRole("button", { name: /src/ })).toBeTruthy();
	});

	it("opens read-only text and saves only after explicit edit", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /hello.txt/ }));
		await waitFor(() => expect(view.getByText("hello")).toBeTruthy());
		expect(view.container.querySelector("pre")?.textContent).toBe("hello\n");
		expect(view.container.querySelector("#markdown-content")).toBeNull();
		expect(view.queryByRole("textbox")).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		await fireEvent.input(view.getByRole("textbox"), { target: { value: "changed\n" } });
		await fireEvent.click(view.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(files.get("src/hello.txt")).toBe("changed\n"));
		expect(view.queryByRole("textbox")).toBeNull();
	});

	it("renders a Markdown file in View, edits source, and returns to rendered View after Save", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /guide.md/ }));
		await waitFor(() => expect(view.container.querySelector("#markdown-content h1")?.textContent).toBe("Guide"));
		expect(view.container.querySelector("#markdown-content strong")?.textContent).toBe("Important");
		expect(view.queryByRole("textbox")).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		expect((view.getByRole("textbox") as HTMLTextAreaElement).value).toBe("# Guide\n\n**Important** note.\n");
		await fireEvent.input(view.getByRole("textbox"), { target: { value: "## Revised\n\nSaved from mobile.\n" } });
		await fireEvent.click(view.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(view.container.querySelector("#markdown-content h2")?.textContent).toBe("Revised"));
		expect(files.get("src/guide.md")).toBe("## Revised\n\nSaved from mobile.\n");
		expect(view.queryByRole("textbox")).toBeNull();
	});

	it("serves nested Markdown images from the file directory while retaining absolute image sources", async () => {
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/repo-one", worktreePath: "/repo-one" }}
				initialLink={{ candidate: "src/deep/guide.md" }}
			/>
		));
		await waitFor(() => expect(view.container.querySelectorAll("#markdown-content img").length).toBe(3));
		const images = view.container.querySelectorAll<HTMLImageElement>("#markdown-content img");
		const local = new URL(images[0].src);
		expect(local.pathname).toBe("/fs/markdown-image");
		expect(local.searchParams.get("repoPath")).toBe("/repo-one");
		expect(local.searchParams.get("file")).toBe("src/deep/images/diagram.png");
		expect(images[1].getAttribute("src")).toBe("https://example.com/logo.png");
		expect(images[2].getAttribute("src")).toBe("data:image/png;base64,AAAA");
	});

	// Catches: an absolute image path being joined to the file directory ("src//repo-one/..."), a 404.
	it("maps an absolute image inside the repository to its repo-relative path and leaves one outside alone", async () => {
		linkedTarget.value = { absolute_path: "/repo-one/src/abs.md", is_directory: false };
		const view = render(() => (
			<FilesScreen
				initialRepo={{ cwd: "/repo-one", worktreePath: "/repo-one" }}
				initialLink={{ candidate: "src/abs.md" }}
			/>
		));
		await waitFor(() => expect(view.container.querySelectorAll("#markdown-content img").length).toBe(2));
		const [inside, outside] = view.container.querySelectorAll<HTMLImageElement>("#markdown-content img");
		const url = new URL(inside.src);
		expect(url.pathname).toBe("/fs/markdown-image");
		expect(url.searchParams.get("file")).toBe("src/images/in.png");
		expect(outside.getAttribute("src")).toBe("/elsewhere/out.png");
	});

	it("refuses large and binary files without offering an editor", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /huge.txt/ }));
		expect(view.getByText(/too large/i)).toBeTruthy();
		expect(calls).not.toContain("fs_read_file");
		await fireEvent.click(view.getByRole("button", { name: /back/i }));
		await fireEvent.click(view.getByRole("button", { name: /image.bin/ }));
		await waitFor(() => expect(view.getByText(/binary or non-text/i)).toBeTruthy());
		expect(view.queryByRole("button", { name: "Edit" })).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: /back/i }));
		await fireEvent.click(view.getByRole("button", { name: /null.dat/ }));
		await waitFor(() => expect(view.getByText(/binary or non-text/i)).toBeTruthy());
		expect(view.queryByRole("textbox")).toBeNull();
	});

	it("keeps the draft when saving fails and restores the original text on cancel", async () => {
		const view = render(() => <FilesScreen />);
		await waitFor(() => expect(view.getByRole("button", { name: /repo-one/ })).toBeTruthy());
		await openRepo(view.getByRole);
		await fireEvent.click(view.getByRole("button", { name: /hello.txt/ }));
		await waitFor(() => expect(view.getByText("hello")).toBeTruthy());
		await fireEvent.click(view.getByRole("button", { name: "Edit" }));
		await fireEvent.input(view.getByRole("textbox"), { target: { value: "unsaved" } });
		failSave = true;
		await fireEvent.click(view.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(view.getByText(/could not save/i)).toBeTruthy());
		expect((view.getByRole("textbox") as HTMLTextAreaElement).value).toBe("unsaved");
		expect(files.get("src/hello.txt")).toBe("hello\n");
		await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
		expect(view.getByText("hello")).toBeTruthy();
	});
});
