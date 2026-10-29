import { readFileSync } from "node:fs";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ChangesTab } from "../../components/GitPanel/ChangesTab";
import { mockInvoke } from "../mocks/tauri";

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { getRevision: () => 0, isGitRepo: () => true },
}));
vi.mock("../../hooks/useSmartPrompts", () => ({
	useSmartPrompts: () => ({ canExecute: () => ({ ok: true }), executeSmartPrompt: vi.fn() }),
}));

let sheet: HTMLStyleElement;
beforeEach(() => {
	mockInvoke.mockResolvedValue({
		staged: [{ path: "src/staged.ts", status: "M", additions: 7, deletions: 2 }],
		unstaged: [{ path: "src/a/very/long/directory/changed-file.ts", status: "M", additions: 16, deletions: 9 }],
		untracked: ["new-file.ts"],
	});
	sheet = document.createElement("style");
	// Happy DOM has no pointer hit testing. Map only the hover pseudo-class to
	// an explicit test marker; the Vite browser fixture checks real hover too.
	// Happy DOM does not match :enabled; :not(:disabled) is equivalent for these buttons.
	sheet.textContent = (
		":root { --fg-secondary: #a0a0a0; --error: #ef4444; }\n" +
		readFileSync("src/components/ui/FormControls.module.css", "utf8") +
		readFileSync("src/components/GitPanel/ChangesTab.module.css", "utf8")
	)
		.replaceAll(":hover", "[data-test-hover]")
		.replaceAll(":enabled", ":not(:disabled)");
	document.head.append(sheet);
});
afterEach(() => {
	cleanup();
	sheet.remove();
	vi.clearAllMocks();
});

async function mount() {
	const view = render(() => <ChangesTab repoPath="/layout" onOpenDiff={vi.fn()} />);
	await waitFor(() => expect(view.container.querySelectorAll(".fileEntry")).toHaveLength(3));
	return view;
}

describe("ChangesTab layout", () => {
	it.each(["rest", "hover", "keyboard"])(
		"hides stats when %s exposes actions — catches focused stats bleeding through buttons",
		async (mode) => {
			const { container } = await mount();
			const row = container.querySelectorAll<HTMLElement>(".fileEntry")[1];
			const stats = row.querySelector(".diffStats")!;
			const actions = row.querySelector(".actions")!;
			if (mode === "hover") row.setAttribute("data-test-hover", "");
			else if (mode === "keyboard") {
				fireEvent.keyDown(container.firstElementChild!, { key: "ArrowDown" });
				fireEvent.keyDown(container.firstElementChild!, { key: "ArrowDown" });
			}
			// Read computed styles only after the state change: Happy DOM caches
			// grandparent selector matches across mutations once styles have been read.
			if (mode === "rest") {
				expect(getComputedStyle(actions).visibility).toBe("hidden");
				expect(getComputedStyle(stats).visibility).not.toBe("hidden");
			} else {
				expect(getComputedStyle(actions).visibility).toBe("visible");
				expect(getComputedStyle(stats).visibility).toBe("hidden");
			}
		},
	);

	it("reserves a non-overlay slot for every file kind — catches paths extending underneath actions", async () => {
		const { container } = await mount();
		for (const row of container.querySelectorAll(".fileEntry")) {
			const actions = row.querySelector(".actions")!;
			expect(["absolute", "fixed"]).not.toContain(getComputedStyle(actions).position);
			expect(actions.parentElement).not.toBe(row);
			expect(getComputedStyle(actions.parentElement!).flexShrink).toBe("0");
			expect(getComputedStyle(row.querySelector(".filePath")!).textOverflow).toBe("ellipsis");
		}
	});

	it("keeps headers short and commit compact — catches wrapped unstaged label and oversized primary commit", async () => {
		const { container, getByTitle } = await mount();
		expect([...container.querySelectorAll(".sectionLabel")].map((el) => el.textContent)).toEqual(["Staged", "Changes"]);
		expect(getComputedStyle(container.querySelector(".sectionLabel")!).whiteSpace).toBe("nowrap");
		expect(getByTitle("Commit staged changes").classList.contains("compact")).toBe(true);
		expect(getByTitle("Generate commit message").classList.contains("compact")).toBe(false);
	});

	it("keeps discard neutral until its own hover and retains confirmation — catches permanently red discard", async () => {
		const { getAllByTitle, getByText } = await mount();
		const discard = getAllByTitle("Discard changes")[0];
		expect(getComputedStyle(discard).color).toBe("#a0a0a0");
		discard.setAttribute("data-test-hover", "");
		expect(getComputedStyle(discard).color).toBe("#ef4444");
		fireEvent.click(discard);
		expect(
			getByText('Discard all changes to "src/a/very/long/directory/changed-file.ts"?', { exact: false }),
		).toBeTruthy();
		expect(mockInvoke.mock.calls.some(([cmd]) => cmd === "git_discard_files")).toBe(false);
	});
});
