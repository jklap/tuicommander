import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NewSessionSheet } from "../components/NewSessionSheet";

vi.mock("../../transport", () => ({
	rpc: vi.fn().mockResolvedValue({}),
}));
vi.mock("../../stores/toasts", () => ({
	toastsStore: { add: vi.fn() },
}));

import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

const REPOS = ["/home/user/project-a", "/home/user/project-b"];

describe("NewSessionSheet", () => {
	it("searches repositories, spawns the chosen agent, and opens its session", async () => {
		vi.mocked(rpc).mockResolvedValueOnce("new-session-id");
		const onCreated = vi.fn();
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} onCreated={onCreated} />);
		await fireEvent.change(view.getByRole("combobox", { name: "Agent" }), { target: { value: "codex" } });
		await fireEvent.input(view.getByRole("searchbox", { name: "Search repositories" }), {
			target: { value: "project-b" },
		});
		expect(view.queryByRole("button", { name: /project-a/ })).toBeNull();
		await fireEvent.click(view.getByRole("button", { name: /project-b/ }));
		expect(rpc).toHaveBeenCalledWith("spawn_agent", {
			pty_config: { cwd: "/home/user/project-b", rows: 24, cols: 80, user_initiated: true },
			agent_config: { cwd: "/home/user/project-b", agent_type: "codex", prompt: "", print_mode: false, args: [] },
		});
		expect(onCreated).toHaveBeenCalledWith("new-session-id");
		expect(onDismiss).toHaveBeenCalled();
	});

	it("closes with an explicit X", async () => {
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} onCreated={() => {}} />);
		await fireEvent.click(view.getByRole("button", { name: "Close new session" }));
		expect(onDismiss).toHaveBeenCalled();
	});

	it("keeps the sheet open after spawn failure", async () => {
		vi.mocked(rpc).mockRejectedValueOnce(new Error("Agent unavailable"));
		const onDismiss = vi.fn();
		const onCreated = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} onCreated={onCreated} />);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(view.getByRole("dialog", { name: "New session" })).toBeTruthy();
		expect(onCreated).not.toHaveBeenCalled();
		expect(onDismiss).not.toHaveBeenCalled();
	});

	it("does not start a second agent while a spawn is pending", async () => {
		let finish: ((id: string) => void) | undefined;
		vi.mocked(rpc).mockImplementationOnce(
			() =>
				new Promise<string>((resolve) => {
					finish = resolve;
				}) as ReturnType<typeof rpc>,
		);
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(view.getByRole("button", { name: /project-a/ })).toHaveProperty("disabled", true);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).toHaveBeenCalledTimes(1);
		finish?.("created-session");
		await waitFor(() => expect(onDismiss).toHaveBeenCalledOnce());
	});
	it("renders a button for each repo", () => {
		const { container } = render(() => <NewSessionSheet repos={REPOS} onDismiss={() => {}} />);
		const buttons = container.querySelectorAll("button[class*='repoItem']");
		expect(buttons.length).toBe(2);
		expect(buttons[0].textContent).toContain("project-a");
		expect(buttons[1].textContent).toContain("project-b");
	});

	it("starts the selected agent in the selected repository", async () => {
		vi.mocked(rpc).mockResolvedValueOnce("created-session");
		const onDismiss = vi.fn();
		const view = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />);
		await fireEvent.click(view.getByRole("button", { name: /project-a/ }));
		expect(rpc).toHaveBeenCalledWith("spawn_agent", {
			pty_config: { cwd: "/home/user/project-a", rows: 24, cols: 80, user_initiated: true },
			agent_config: { cwd: "/home/user/project-a", agent_type: "claude", prompt: "", print_mode: false, args: [] },
		});
		expect(onDismiss).toHaveBeenCalled();
	});

	it("calls onDismiss when backdrop is clicked", async () => {
		const onDismiss = vi.fn();
		const { container } = render(() => <NewSessionSheet repos={REPOS} onDismiss={onDismiss} />);
		const backdrop = container.firstElementChild as HTMLElement;
		await fireEvent.click(backdrop);
		expect(onDismiss).toHaveBeenCalled();
	});

	it("shows empty state when no repos", () => {
		const { container } = render(() => <NewSessionSheet repos={[]} onDismiss={() => {}} />);
		expect(container.textContent).toContain("No repositories configured");
	});
});
