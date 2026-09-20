import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DirEntry } from "../../types/fs";

const { mockRpc } = vi.hoisted(() => ({ mockRpc: vi.fn() }));

vi.mock("../../transport", () => ({ rpc: mockRpc }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../i18n", () => ({ t: (_key: string, fallback: string) => fallback }));

import { RemoteRepoPicker } from "../../components/RemoteRepoPicker/RemoteRepoPicker";

const dir = (name: string): DirEntry => ({
	name,
	path: name,
	is_dir: true,
	size: 0,
	modified_at: 0,
	git_status: "",
	is_ignored: false,
});

const file = (name: string): DirEntry => ({ ...dir(name), is_dir: false });

/** Listing keyed by the path asked for, so a test states the remote tree once. */
function serveTree(tree: Record<string, DirEntry[]>) {
	mockRpc.mockImplementation((command: string, args: Record<string, unknown>) => {
		if (command !== "list_directory") throw new Error(`unexpected command ${command}`);
		const path = args.repoPath as string;
		const entries = tree[path];
		if (!entries) return Promise.reject(new Error(`Failed to resolve repo path: ${path}`));
		return Promise.resolve(entries);
	});
}

/**
 * Each test gets its own connection id on purpose. The picker remembers where a
 * machine was last left, so a shared id would make one test's walk decide the
 * next test's starting directory — and the failure would read as a broken
 * listing rather than as the memory doing its job.
 */
let nextConnection = 0;
function open(onConfirm = vi.fn(), onClose = vi.fn()) {
	const connectionId = `conn-${++nextConnection}`;
	const utils = render(() => (
		<RemoteRepoPicker
			visible={true}
			connectionId={connectionId}
			connectionName="mac-mint"
			onClose={onClose}
			onConfirm={onConfirm}
		/>
	));
	return { ...utils, onConfirm, onClose, connectionId };
}

describe("RemoteRepoPicker", () => {
	beforeEach(() => {
		mockRpc.mockReset();
		// The picker remembers where each machine was left, in module scope. Give
		// every test its own machine id so one test's walk cannot seed the next.
		vi.resetModules();
	});

	it("lists the remote machine's directories, routed to that connection", async () => {
		serveTree({ "/": [dir("home"), dir("etc"), file("vmlinuz")] });
		const { findByText, queryByText, connectionId } = open();

		await findByText("home");
		expect(queryByText("etc")).toBeTruthy();
		// A picker for a *repository* offers directories only: a file can never be
		// the answer, and listing files buries the ones that can.
		expect(queryByText("vmlinuz")).toBeNull();

		expect(mockRpc).toHaveBeenCalledWith("list_directory", { repoPath: "/", subdir: "" }, connectionId);
	});

	it("walks into a directory and confirms the path it is showing", async () => {
		serveTree({
			"/": [dir("home")],
			"/home": [dir("stefano")],
			"/home/stefano": [dir("Gits")],
		});
		const { findByText, onConfirm } = open();

		fireEvent.click(await findByText("home"));
		fireEvent.click(await findByText("stefano"));
		await findByText("Gits");

		fireEvent.click(await findByText("Add This Folder"));
		// The confirmed value is the absolute path *on the remote machine*, which is
		// the only spelling `get_repo_info` can resolve there.
		expect(onConfirm).toHaveBeenCalledWith("/home/stefano");
	});

	it("goes back up without leaving the machine's root", async () => {
		serveTree({ "/": [dir("home")], "/home": [dir("stefano")] });
		const { findByText, queryByText } = open();

		// At the root there is nothing above: no ".." row to click into a path the
		// daemon cannot resolve.
		await findByText("home");
		expect(queryByText("..")).toBeNull();

		fireEvent.click(await findByText("home"));
		fireEvent.click(await findByText(".."));
		await findByText("home");
		expect(queryByText("..")).toBeNull();
	});

	it("reports the daemon's own failure instead of an empty folder", async () => {
		serveTree({ "/": [dir("home")] });
		const { findByText, container } = open();

		await findByText("home");
		const input = container.querySelector("input") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "/nope" } });
		fireEvent.keyDown(input, { key: "Enter" });

		// A path that is not there and a machine that stopped answering are
		// different failures; rewriting them into "no subfolders" hides both.
		await waitFor(() => expect(container.textContent).toContain("Failed to resolve repo path: /nope"));
	});

	it("jumps to a typed path, because the field is an escape hatch and not a filter", async () => {
		serveTree({ "/": [dir("home")], "/home/stefano/Gits": [dir("tuicommander")] });
		const { findByText, container, onConfirm } = open();

		await findByText("home");
		const input = container.querySelector("input") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "/home/stefano/Gits" } });
		fireEvent.keyDown(input, { key: "Enter" });

		await findByText("tuicommander");
		fireEvent.click(await findByText("Add This Folder"));
		expect(onConfirm).toHaveBeenCalledWith("/home/stefano/Gits");
	});
});
