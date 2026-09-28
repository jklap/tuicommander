import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { workflowRunSignals } from "../../stores/workflowRunSignals";
import { HttpRpcError } from "../../transport";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

const plan = { id: "p1", project: "/repo", title: "Plan A", source: "plans/a.md" };
const story = {
	id: "s1",
	planId: "p1",
	title: "Implement API",
	criteria: ["API works"],
	checked: [false],
	dependencies: [],
	priority: 1,
	origin: "native",
	fileScope: ["src/api.rs"],
	status: "ready",
	revision: 1,
	claimSession: null,
	abandoned: false,
};

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		if (_command === "story_capabilities") return true;
		const action = (args as { action: { action: string } }).action;
		if (action.action === "list_plans") return { type: "plans", value: [plan] };
		if (action.action === "plan_view")
			return { type: "plan_view", value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false } };
		if (action.action === "transition")
			return { type: "story", value: { ...story, status: "in_progress", revision: 2 } };
		throw new Error(`unexpected action ${action.action}`);
	});
});

describe("StoriesDialog", () => {
	it("offers repository plans first and adds one without a typed title", async () => {
		let created = false;
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string; source?: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: created ? [plan] : [] };
			if (action.action === "list_plan_sources")
				return { type: "plan_sources", value: [{ title: "Plan A", source: "plans/a.md" }] };
			if (action.action === "add_plan_source") {
				created = true;
				return { type: "plan", value: plan };
			}
			if (action.action === "plan_view")
				return { type: "plan_view", value: { stories: [], state: "draft", wontFixCount: 0, allCancelled: false } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		fireEvent.click(await screen.findByRole("button", { name: /Plan A/ }));
		await waitFor(() => expect(screen.getByRole("button", { name: "Plan A" })).toBeTruthy());
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "add_plan_source", source: "plans/a.md" },
		});
	});

	it("shows discovered plans immediately when no plan is registered", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [] };
			if (action.action === "list_plan_sources")
				return { type: "plan_sources", value: [{ title: "Agent plan", source: "plans/agent.md" }] };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		expect(await screen.findByRole("button", { name: /Agent plan/ })).toBeTruthy();
	});

	it("rejects an invalid response when adding a discovered plan", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [] };
			if (action.action === "list_plan_sources")
				return { type: "plan_sources", value: [{ title: "Agent plan", source: "plans/agent.md" }] };
			if (action.action === "add_plan_source") return { type: "plans", value: [] };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		fireEvent.click(await screen.findByRole("button", { name: /Agent plan/ }));
		await screen.findByText("Invalid plan response");
	});
	it("moves initial focus into the dialog", async () => {
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		await waitFor(() =>
			expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close Plans and Stories" })),
		);
	});

	it("loads plan and story detail, then starts manual work through the backend", async () => {
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		expect(screen.getByText("API works")).toBeTruthy();
		expect(screen.getByText("src/api.rs")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Start work" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "transition", story_id: "s1", expected_revision: 1, command: "start_manual" },
			}),
		);
	});

	it("shows a recoverable error when loading fails", async () => {
		vi.mocked(invoke).mockResolvedValueOnce(true).mockRejectedValueOnce(new Error("offline"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/offline/);
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		await screen.findByRole("heading", { name: "Implement API" });
	});

	it("explains how to load the missing native stories backend", async () => {
		vi.stubGlobal("__TAURI_INTERNALS__", {});
		try {
			vi.mocked(invoke).mockRejectedValueOnce("Command story_capabilities not found");
			render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
			await screen.findByText(/Restart TUICommander/);
			expect(invoke).toHaveBeenCalledWith("story_capabilities");
		} finally {
			vi.unstubAllGlobals();
		}
	});

	it("preserves an HTTP domain error even when it says a story was not found", async () => {
		vi.mocked(invoke)
			.mockResolvedValueOnce(true)
			.mockRejectedValueOnce(new HttpRpcError("story_action_command", 500, '{"error":"story not found: s1"}'));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/story not found: s1/);
		expect(screen.queryByText(/Restart TUICommander/)).toBeNull();
	});

	it("classifies a missing HTTP capability route by its status", async () => {
		vi.mocked(invoke).mockRejectedValueOnce(new HttpRpcError("story_capabilities", 404, "Not Found"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/Restart TUICommander/);
	});

	it("recognizes an older browser backend serving its HTML fallback", async () => {
		vi.mocked(invoke).mockResolvedValueOnce("<!doctype html><html><body>TUICommander</body></html>");
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/Restart TUICommander/);
		expect(screen.queryByText("Invalid plan response")).toBeNull();
	});

	it("creates a story with criteria and relative file scope", async () => {
		const created = { ...story, id: "s2", title: "Review API" };
		let rows = [story];
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string; input?: unknown } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return { type: "plan_view", value: { stories: rows, state: "active", wontFixCount: 0, allCancelled: false } };
			if (action.action === "create_story") {
				rows = [story, created];
				return { type: "story", value: created };
			}
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "New story" }));
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Review API" } });
		fireEvent.input(screen.getByLabelText("Acceptance criteria, one per line"), {
			target: { value: "Review passes\nNo regressions" },
		});
		fireEvent.input(screen.getByLabelText("File scope, one relative path per line"), {
			target: { value: "src/api.rs" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Create story" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: {
					action: "create_story",
					input: {
						planId: "p1",
						title: "Review API",
						criteria: ["Review passes", "No regressions"],
						priority: 2,
						origin: { type: "native" },
						fileScope: ["src/api.rs"],
					},
				},
			}),
		);
	});

	it("sends indexed criterion changes in the backend enum shape", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: {
						stories: [{ ...story, status: "in_progress" }],
						state: "active",
						wontFixCount: 0,
						allCancelled: false,
					},
				};
			if (action.action === "transition") return { type: "story", value: { ...story, status: "in_progress" } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("checkbox", { name: "API works" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "transition", story_id: "s1", expected_revision: 1, command: { check_criterion: 0 } },
			}),
		);
	});

	it("creates a plan and adds an eligible story dependency", async () => {
		let created = false;
		const dependent = { ...story, id: "s2", title: "Review API" };
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: created ? [plan] : [] };
			if (action.action === "list_plan_sources") return { type: "plan_sources", value: [] };
			if (action.action === "add_plan_source") {
				created = true;
				return { type: "plan", value: plan };
			}
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story, dependent], state: "active", wontFixCount: 0, allCancelled: false },
				};
			if (action.action === "add_dependency") return { type: "story", value: { ...story, dependencies: ["s2"] } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText("Create a plan to begin.");
		fireEvent.click(screen.getByRole("button", { name: "New plan" }));
		fireEvent.click(screen.getByText("Add from path or link"));
		fireEvent.input(screen.getByLabelText("Plan document or link"), { target: { value: "plans/a.md" } });
		fireEvent.click(screen.getByRole("button", { name: "Create plan" }));
		await screen.findByRole("heading", { name: "Implement API" });
		expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "add_plan_source", source: "plans/a.md" },
		});
		fireEvent.change(screen.getByRole("combobox", { name: "Add dependency" }), { target: { value: "s2" } });
		fireEvent.click(screen.getByRole("button", { name: "Add dependency" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "add_dependency", story_id: "s1", dependency_id: "s2", expected_revision: 1 },
			}),
		);
	});

	it("drops a pending dependency choice when another story is selected", async () => {
		const second = { ...story, id: "s2", title: "Review API" };
		const third = { ...story, id: "s3", title: "Ship API" };
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: { stories: [story, second, third], state: "active", wontFixCount: 0, allCancelled: false },
				};
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
		vi.mocked(invoke).mockResolvedValueOnce(true).mockRejectedValueOnce(new Error("offline"));
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText(/offline/);
		expect(appLogger.warn).toHaveBeenCalledWith("store", expect.stringContaining("Stories"), expect.anything());
	});

	it("shows cancelled dependencies and waives only the direct cancelled edge", async () => {
		const cancelled = { ...story, id: "s2", title: "Cancelled API", status: "wontfix", abandoned: true };
		const intermediate = {
			...story,
			id: "s3",
			title: "Intermediate API",
			status: "backlog",
			dependencies: ["s2"],
			abandoned: true,
		};
		const dependent = {
			...story,
			id: "s4",
			title: "Ship API",
			status: "backlog",
			dependencies: ["s2", "s3"],
			abandoned: true,
		};
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return {
					type: "plan_view",
					value: {
						stories: [cancelled, intermediate, dependent],
						state: "active",
						wontFixCount: 1,
						allCancelled: false,
					},
				};
			if (action.action === "remove_dependency") return { type: "story", value: dependent };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Cancelled API" });
		expect(screen.getByText("1 won't fix")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: /Ship API/ }));
		await screen.findByRole("heading", { name: "Ship API" });
		expect(screen.getAllByText(/abandoned/i)).toHaveLength(2);
		const remove = screen.getByRole("button", { name: /Remove.*Cancelled API/ });
		fireEvent.click(remove);
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("story_action_command", {
				project: "/repo",
				action: { action: "remove_dependency", story_id: "s4", dependency_id: "s2", expected_revision: 1 },
			}),
		);
		expect(screen.queryByRole("button", { name: /Remove.*Intermediate API/ })).toBeNull();
	});

	it("renders the service cancellation summary without recounting story statuses", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view")
				return { type: "plan_view", value: { stories: [story], state: "done", wontFixCount: 1, allCancelled: true } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByText("All cancelled");
	});


	it("offers browser approval and sends the selected transition", async () => {
		const environment = globalThis as Record<string, unknown>;
		const priorShim = environment.__TAURI_SHIM__;
		environment.__TAURI_SHIM__ = true;
		try {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view") return { type: "plan_view", value: { stories: [{ ...story, status: "review" }], state: "active", wontFixCount: 0, allCancelled: false } };
			if (action.action === "transition") return { type: "story", value: { ...story, status: "done" } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Approve" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "transition", story_id: "s1", expected_revision: 1, command: "approve" },
		}));
		} finally {
			environment.__TAURI_SHIM__ = priorShim;
		}
	});

	it("offers desktop review actions and sends the selected transition", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			if (_command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view") return { type: "plan_view", value: { stories: [{ ...story, status: "review" }], state: "active", wontFixCount: 0, allCancelled: false } };
			if (action.action === "transition") return { type: "story", value: { ...story, status: "review" } };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
		await screen.findByRole("heading", { name: "Implement API" });
		fireEvent.click(screen.getByRole("button", { name: "Approve" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "transition", story_id: "s1", expected_revision: 1, command: "approve" },
		}));
		await waitFor(() => expect((screen.getByRole("button", { name: "Request changes" }) as HTMLButtonElement).disabled).toBe(false));
		fireEvent.click(screen.getByRole("button", { name: "Request changes" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_action_command", {
			project: "/repo",
			action: { action: "transition", story_id: "s1", expected_revision: 1, command: "reject_review" },
		}));
	});


	it("opens the persisted run timeline for the selected plan", async () => {
		const run = {
			id: "r1", planId: "p1", status: "paused", sequence: 4, startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }], attempts: [],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities") return true;
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
			if (action.action === "plan_view") return { type: "plan_view", value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false } };
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

	it("records a human answer to a paused workflow request", async () => {
		const run = {
			id: "r1", planId: "p1", status: "paused", sequence: 4, startedMs: 1_000,
			stories: [{ storyId: "s1", accepted: false }],
			attempts: [{ id: "a1", storyId: "s1", nodeId: "implement", state: "reported",
				outcome: "needs_input", inputAnswer: null,
				report: { inputRequest: { question: "Use A or B?", options: ["A", "B"] } } }],
		};
		vi.mocked(invoke).mockImplementation(async (command, args) => {
			if (command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events") return { type: "events", value: [] };
				if (action.action === "command") return { type: "receipt", value: { snapshot: run } };
			}
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view") return { type: "plan_view", value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false } };
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
			if (command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (command === "workflow_run_action") {
				if (action.action === "list_plan_runs") return { type: "runs", value: [run] };
				if (action.action === "get") return { type: "snapshot", value: run };
				if (action.action === "events") return { type: "events", value: [] };
				if (action.action === "command") return { type: "receipt", value: { snapshot: { ...run, status: "running", sequence: 6 } } };
			}
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view") return { type: "plan_view", value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false } };
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
			if (command === "story_capabilities") return true;
			const action = (args as { action: { action: string } }).action;
			if (command === "workflow_definition_action" && action.action === "list_drafts") return { type: "drafts", value: [{
				id: "f1", project: "/repo", name: "Story delivery", kind: "story",
				graph: { nodes: [{ id: "start", kind: { type: "start" } }, { id: "end", kind: { type: "end" } }], edges: [] },
				draftRevision: 1, latestPublishedRevision: 1, builtinKey: "story_delivery",
			}] };
			if (action.action === "list_plans") return { type: "plans", value: [plan] };
			if (action.action === "plan_view") return { type: "plan_view", value: { stories: [story], state: "active", wontFixCount: 0, allCancelled: false } };
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
