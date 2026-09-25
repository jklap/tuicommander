import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { workflowRunSignals } from "../../stores/workflowRunSignals";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plan = { id: "p1", project: "/repo", title: "Plan A", source: "plans/a.md" };
const story = {
	id: "s1", planId: "p1", title: "Implement API", criteria: ["API works"], checked: [false],
	dependencies: [], priority: 1, origin: "native", fileScope: ["src/api.rs"],
	status: "ready", revision: 1, claimSession: null,
};

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		const action = (args as { action: { action: string } }).action;
		if (action.action === "list_plans") return { type: "plans", value: [plan] };
		if (action.action === "list_stories") return { type: "stories", value: [story] };
		if (action.action === "plan_state") return { type: "plan_state", value: "active" };
		if (action.action === "transition") return { type: "story", value: { ...story, status: "in_progress", revision: 2 } };
		throw new Error(`unexpected action ${action.action}`);
	});
});

describe("StoriesDialog", () => {
	it("shows the browser approval limitation instead of an action that cannot succeed", async () => {
		const environment = globalThis as Record<string, unknown>;
		const priorShim = environment.__TAURI_SHIM__;
		environment.__TAURI_SHIM__ = true;
		try {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "list_stories") return { type: "stories", value: [{ ...story, status: "review" }] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		expect(screen.queryByRole("button", { name: "Approve" })).toBeNull();
		expect(screen.getByText("Approval requires the desktop app.")).toBeTruthy();
		} finally {
			environment.__TAURI_SHIM__ = priorShim;
		}
	});
	it("loads plan and story detail, then starts manual work through the backend", async () => {
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		expect(screen.getByText("API works")).toBeTruthy();
		expect(screen.getByText("src/api.rs")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Start work" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "transition", story_id: "s1", expected_revision: 1, command: "start_manual" },
		}));
	});

	it("shows a recoverable error when loading fails", async () => {
		vi.mocked(invoke).mockRejectedValueOnce(new Error("offline"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/offline/);
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		await screen.findByRole("heading", { name: "Implement API" });
	});

	it("creates a story with criteria and relative file scope", async () => {
		const created = { ...story, id: "s2", title: "Review API" };
		let rows = [story];
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			const action = (args as { action: { action: string; input?: unknown } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			if (action.action === "list_stories") return { type: "stories", value: rows };
			if (action.action === "create_story") { rows = [story, created]; return { type: "story", value: created }; }
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "New story" }));
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Review API" } });
		fireEvent.input(screen.getByLabelText("Acceptance criteria, one per line"), { target: { value: "Review passes\nNo regressions" } });
		fireEvent.input(screen.getByLabelText("File scope, one relative path per line"), { target: { value: "src/api.rs" } });
		fireEvent.click(screen.getByRole("button", { name: "Create story" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "create_story", input: {
				planId: "p1", title: "Review API", criteria: ["Review passes", "No regressions"],
				priority: 2, origin: { type: "native" }, fileScope: ["src/api.rs"],
			} },
		}));
	});

	it("sends indexed criterion changes in the backend enum shape", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			if (action.action === "list_stories") return { type: "stories", value: [{ ...story, status: "in_progress" }] };
			if (action.action === "transition") return { type: "story", value: { ...story, status: "in_progress" } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("checkbox", { name: "API works" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "transition", story_id: "s1", expected_revision: 1, command: { check_criterion: 0 } },
		}));
	});

	it("creates a plan and adds an eligible story dependency", async () => {
		let created = false;
		const dependent = { ...story, id: "s2", title: "Review API" };
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: created ? [plan] : [] };
			if (action.action === "create_plan") { created = true; return { type: "plan", value: plan }; }
			if (action.action === "list_stories") return { type: "stories", value: [story, dependent] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			if (action.action === "add_dependency") return { type: "story", value: { ...story, dependencies: ["s2"] } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText("Create a plan to begin.");
		fireEvent.click(screen.getByRole("button", { name: "New plan" }));
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Plan A" } });
		fireEvent.input(screen.getByLabelText("Plan document or link"), { target: { value: "plans/a.md" } });
		fireEvent.click(screen.getByRole("button", { name: "Create plan" }));
		await screen.findByRole("heading", { name: "Implement API" });
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo", action: { action: "create_plan", title: "Plan A", source: "plans/a.md" },
		});
		fireEvent.change(screen.getByRole("combobox", { name: "Add dependency" }), { target: { value: "s2" } });
		fireEvent.click(screen.getByRole("button", { name: "Add dependency" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo", action: { action: "add_dependency", story_id: "s1", dependency_id: "s2", expected_revision: 1 },
		}));
	});

	it("opens the persisted run timeline for the selected plan", async () => {
		const run = {
			id: "r1", planId: "p1", status: "paused", sequence: 4, startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }], attempts: [],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			const action = (args as { action: { action: string; after_sequence?: number } }).action;
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events") return { type: "events", value: action.after_sequence === 4
					? [] : action.after_sequence === 3
					? [{ sequence: 4, atMs: 1_200, kind: { type: "resumed" } }]
					: [
						{ sequence: 1, atMs: 1_000, kind: { type: "started" } },
						{ sequence: 3, atMs: 1_100, kind: { type: "paused" } },
					] };
			}
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "list_stories") return { type: "stories", value: [story] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo/worktree" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		await screen.findByRole("region", { name: "Run timeline" });
		await screen.findByText("started");
		expect(screen.getAllByText("paused").length).toBeGreaterThan(0);
		expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
			project: "/repo/worktree", action: { action: "list_plan_runs", plan_id: "p1", limit: 20 },
		});
		fireEvent.click(screen.getByRole("button", { name: "Load more events" }));
		await screen.findByText("resumed");
		expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
			project: "/repo/worktree", action: { action: "events", run_id: "r1", after_sequence: 3, limit: 100 },
		});
		const reads = vi.mocked(invoke).mock.calls.filter(([command, args]) => command === "workflow_run_action" && (args as { action: { action: string } }).action.action === "get").length;
		workflowRunSignals.accept({ repo_path: "/repo", payload: { runId: "r1", sequence: 5 } });
		await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command, args]) => command === "workflow_run_action" && (args as { action: { action: string } }).action.action === "get").length).toBeGreaterThan(reads));
	});
	it("drops a pending dependency choice when another story is selected", async () => {
		const second = { ...story, id: "s2", title: "Review API" };
		const third = { ...story, id: "s3", title: "Ship API" };
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "list_stories") return { type: "stories", value: [story, second, third] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.change(screen.getByRole("combobox", { name: "Add dependency" }), { target: { value: "s3" } });
		fireEvent.click(screen.getByRole("button", { name: /Review API/ }));
		await screen.findByRole("heading", { name: "Review API" });
		// s3 is also a valid candidate for s2: a leaked choice would stay armed for a story the user never chose it for.
		expect((screen.getByRole("combobox", { name: "Add dependency" }) as HTMLSelectElement).value).toBe("");
		expect((screen.getByRole("button", { name: "Add dependency" }) as HTMLButtonElement).disabled).toBe(true);
	});

	it("logs a failed story action as well as showing it", async () => {
		vi.mocked(invoke).mockRejectedValueOnce(new Error("offline"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/offline/);
		expect(appLogger.warn).toHaveBeenCalledWith("store", expect.stringContaining("Stories"), expect.anything());
	});
	it("records a human answer to a paused workflow request", async () => {
		const run = {
			id: "r1", planId: "p1", status: "paused", sequence: 4, startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }],
			attempts: [{ id: "a1", storyId: "s1", nodeId: "implement", state: "reported",
				outcome: "needs_input", inputAnswer: null,
				report: { inputRequest: { question: "Use A or B?", options: ["A", "B"] } } }],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events") return { type: "events", value: [] };
				if (action.action === "command") return { type: "receipt", value: { snapshot: run } };
			}
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "list_stories") return { type: "stories", value: [story] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		await screen.findByText("Use A or B?");
		fireEvent.input(screen.getByRole("textbox", { name: "Answer" }), { target: { value: "A" } });
		fireEvent.click(screen.getByRole("button", { name: "Record answer" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
			project: "/repo", action: {
				action: "command", run_id: "r1", expected_sequence: 4,
				command_id: expect.any(String),
				command: { action: "answer_input", attempt_id: "a1", answer: "A" },
			},
		}));
	});

	it("resumes a paused run after the answer is recorded", async () => {
		const run = {
			id: "r1", planId: "p1", status: "paused", sequence: 5, startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }],
			attempts: [{ id: "a1", storyId: "s1", nodeId: "implement", state: "reported",
				outcome: "needs_input", inputAnswer: "A",
				report: { inputRequest: { question: "Use A or B?", options: [] } } }],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events") return { type: "events", value: [] };
				if (action.action === "command") return { type: "receipt", value: { snapshot: { ...run, status: "running", sequence: 6 } } };
			}
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "list_stories") return { type: "stories", value: [story] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Run history" }));
		fireEvent.click(await screen.findByRole("button", { name: "Resume run" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("workflow_run_action", {
			project: "/repo", action: {
				action: "command", run_id: "r1", expected_sequence: 5,
				command_id: expect.any(String), command: { action: "resume" },
			},
		}));
	});

	it("opens the project workflow designer", async () => {
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (command === "workflow_definition_action" && action.action === "list_drafts") return { type: "drafts", value: [{
				id: "f1", project: "/repo", name: "Story delivery", kind: "story",
				graph: { nodes: [{ id: "start", kind: { type: "start" } }, { id: "end", kind: { type: "end" } }], edges: [] },
				draftRevision: 1, latestPublishedRevision: 1, builtinKey: "story_delivery",
			}] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "list_stories") return { type: "stories", value: [story] };
			if (action.action === "plan_state") return { type: "plan_state", value: "active" };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Designer" }));
		await screen.findByRole("heading", { name: "Workflow designer" });
		expect(invoke).toHaveBeenCalledWith("workflow_definition_action", {
			project: "/repo", action: { action: "list_drafts" },
		});
	});
});
