import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

describe("useRepoPickerDialog", () => {
	let useRepoPickerDialog: typeof import("../../hooks/useRepoPickerDialog").useRepoPickerDialog;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

		useRepoPickerDialog = (await import("../../hooks/useRepoPickerDialog")).useRepoPickerDialog;
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		repositoriesStore._testSetHydrated(true);
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
	});

	it("has no dialog state before chooseRepoForPath is called", () => {
		const dialog = useRepoPickerDialog();
		expect(dialog.dialogState()).toBeNull();
	});

	it("sets dialogState with the path and the current repo list", async () => {
		await testInScope(async () => {
			repositoriesStore.add({ path: "/repo-a", displayName: "repo-a" });
			const dialog = useRepoPickerDialog();

			const promise = dialog.chooseRepoForPath("/somewhere");

			expect(dialog.dialogState()).toEqual({
				path: "/somewhere",
				repos: [{ path: "/repo-a", displayName: "repo-a" }],
			});

			dialog.handleClose();
			await promise;
		});
	});

	it("resolves with {kind: 'repo', repoPath} on handleChooseRepo, and clears dialogState", async () => {
		await testInScope(async () => {
			const dialog = useRepoPickerDialog();
			const promise = dialog.chooseRepoForPath("/somewhere");

			dialog.handleChooseRepo("/repo-a");
			const choice = await promise;

			expect(choice).toEqual({ kind: "repo", repoPath: "/repo-a" });
			expect(dialog.dialogState()).toBeNull();
		});
	});

	it("resolves with {kind: 'register'} on handleRegister", async () => {
		await testInScope(async () => {
			const dialog = useRepoPickerDialog();
			const promise = dialog.chooseRepoForPath("/somewhere");

			dialog.handleRegister();
			expect(await promise).toEqual({ kind: "register" });
		});
	});

	it("resolves with {kind: 'unattached'} on handleUnattached", async () => {
		await testInScope(async () => {
			const dialog = useRepoPickerDialog();
			const promise = dialog.chooseRepoForPath("/somewhere");

			dialog.handleUnattached();
			expect(await promise).toEqual({ kind: "unattached" });
		});
	});

	it("resolves with null on handleClose (cancel)", async () => {
		await testInScope(async () => {
			const dialog = useRepoPickerDialog();
			const promise = dialog.chooseRepoForPath("/somewhere");

			dialog.handleClose();
			expect(await promise).toBeNull();
		});
	});

	it("queues concurrent requests instead of dropping earlier ones", async () => {
		await testInScope(async () => {
			const dialog = useRepoPickerDialog();
			const first = dialog.chooseRepoForPath("/first");
			const second = dialog.chooseRepoForPath("/second");

			// Only the head of the queue is shown.
			expect(dialog.dialogState()?.path).toBe("/first");

			dialog.handleUnattached();
			expect(await first).toEqual({ kind: "unattached" });

			// The second request is now shown and can be answered independently.
			expect(dialog.dialogState()?.path).toBe("/second");
			dialog.handleRegister();
			expect(await second).toEqual({ kind: "register" });

			expect(dialog.dialogState()).toBeNull();
		});
	});
});
