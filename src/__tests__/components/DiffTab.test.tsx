import { readFileSync } from "node:fs";
import path from "node:path";
import { render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

// @git-diff-view/solid requires a real Canvas for text measurement, which
// jsdom/happy-dom don't provide (see DiffViewer.test.tsx's own note), and its
// synchronous rendering behavior in this environment is otherwise unreliable
// to assert against. DiffViewer is stubbed here so every test below is
// deterministic and exercises only DiffTab's OWN logic: data loading, the
// stale-response guard, revision reactivity, one-sided-diff mode forcing, and
// the large-diff gate. Real diff rendering is covered by DiffViewer.test.tsx;
// hunk-click/line-drag-driven paths are covered by useLineSelection.test.ts
// and sendDiffComment.test.ts, which don't need a rendered DiffViewer at all.
const h = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: h.invoke }));
vi.mock("../../components/ui/DiffViewer", () => ({
	DiffViewer: (props: { diff: string }) => <div data-testid="diff-stub">{props.diff}</div>,
}));

import { DiffTab } from "../../components/DiffTab/DiffTab";
import { repositoriesStore } from "../../stores/repositories";

const REPO = "/repo";
const FILE = "src/main.rs";

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function callsTo(command: string): unknown[][] {
	return h.invoke.mock.calls.filter((args) => args[0] === command);
}

describe("DiffTab data loading", () => {
	beforeEach(() => {
		h.invoke.mockReset();
		h.invoke.mockResolvedValue("");
	});

	it("loads the file diff with repo/file/scope/untracked on mount", async () => {
		render(() => <DiffTab tabId="t1" repoPath={REPO} filePath={FILE} scope="staged" untracked={false} />);
		await settle();
		const calls = callsTo("get_file_diff");
		expect(calls).toHaveLength(1);
		expect(calls[0][1]).toMatchObject({ path: REPO, file: FILE, scope: "staged", untracked: undefined });
	});

	it("a stale earlier fetch never overwrites a newer one (diffGen guard)", async () => {
		let resolveFirst: (v: string) => void = () => {};
		let getFileDiffCalls = 0;
		// Keyed to the specific command (not a bare `mockImplementationOnce` queue)
		// so an unrelated invoke() call elsewhere can't consume the wrong slot.
		h.invoke.mockImplementation((cmd: string) => {
			if (cmd !== "get_file_diff") return Promise.resolve(undefined);
			getFileDiffCalls += 1;
			if (getFileDiffCalls === 1) {
				return new Promise((resolve) => {
					resolveFirst = resolve;
				});
			}
			return Promise.resolve("SECOND");
		});
		const { getByTestId } = render(() => <DiffTab tabId="t1" repoPath={REPO} filePath={FILE} />);
		await settle();

		// Trigger a second, faster load before the first resolves.
		repositoriesStore.bumpRevision(REPO);
		await settle();

		// Now let the FIRST (stale) fetch resolve — it must not clobber the second's result.
		resolveFirst("FIRST-STALE");
		await settle();

		expect(getByTestId("diff-stub").textContent).toBe("SECOND");
	});

	it("a working-tree revision bump re-fetches the diff", async () => {
		render(() => <DiffTab tabId="t1" repoPath={REPO} filePath={FILE} />);
		await settle();
		const initial = callsTo("get_file_diff").length;

		repositoriesStore.bumpRevision(REPO);
		await settle();
		expect(callsTo("get_file_diff").length).toBe(initial + 1);
	});

	it("forces unified mode and disables split/scroll for a one-sided (new file) diff", async () => {
		h.invoke.mockResolvedValue("diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n");
		const { container } = render(() => <DiffTab tabId="t1" repoPath={REPO} filePath={FILE} />);
		await settle();

		const splitBtn = container.querySelector('[title="Side-by-side"]') as HTMLButtonElement | null;
		const scrollBtn = container.querySelector('[title="All files"]') as HTMLButtonElement | null;
		expect(splitBtn?.disabled).toBe(true);
		expect(scrollBtn?.disabled).toBe(true);
	});

	it("does not force unified mode / disable other views for a normal two-sided diff", async () => {
		h.invoke.mockResolvedValue("diff --git a/f.ts b/f.ts\n@@ -1,1 +1,1 @@\n-old\n+new\n");
		const { container } = render(() => <DiffTab tabId="t1" repoPath={REPO} filePath={FILE} />);
		await settle();

		const splitBtn = container.querySelector('[title="Side-by-side"]') as HTMLButtonElement | null;
		expect(splitBtn?.disabled).toBe(false);
	});

	it("shows a large-diff notice above the line-count threshold, and the escape hatch reveals the diff", async () => {
		const bigDiff = Array.from({ length: 3500 }, (_, i) => `+line ${i}`).join("\n");
		h.invoke.mockResolvedValue(bigDiff);
		const { getByText, queryByTestId } = render(() => <DiffTab tabId="t1" repoPath={REPO} filePath={FILE} />);
		await settle();

		expect(getByText(/This diff is large/)).toBeTruthy();
		expect(queryByTestId("diff-stub")).toBeNull();

		getByText("Render anyway").click();
		await settle();
		expect(queryByTestId("diff-stub")).not.toBeNull();
	});
});

describe("DiffTab discard confirm defaults to Cancel (regression)", () => {
	// The revert `ConfirmDialog` can only be driven open via a real hunk button
	// or a line-drag selection, both of which need a rendered @git-diff-view
	// hunk DOM that jsdom/happy-dom cannot produce (no Canvas). This is a
	// direct guard against the exact regression this file's extraction found:
	// a `ConfirmDialog` with no `defaultButton` defaults to "confirm", so
	// pressing Enter on "Discard this change?" destroyed work.
	it('passes defaultButton="cancel" to the revert ConfirmDialog', () => {
		const src = readFileSync(path.resolve(__dirname, "../../components/DiffTab/DiffTab.tsx"), "utf-8");
		const dialogBlock = src.slice(src.indexOf("<ConfirmDialog"), src.indexOf("<ConfirmDialog") + 400);
		expect(dialogBlock).toContain('defaultButton="cancel"');
	});
});
