// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FilesScreen } from "../screens/FilesScreen";

vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

const files = new Map([
	["src/hello.txt", "hello\n"],
	["src/guide.md", "# Guide\n\n**Important** note.\n"],
	["src/null.dat", "abc\0def"],
]);
const calls: string[] = [];
let failSave = false;

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
			return [{ name: "src", path: "src", is_dir: true, size: 0 }];
		}
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
});

describe("FilesScreen", () => {
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
