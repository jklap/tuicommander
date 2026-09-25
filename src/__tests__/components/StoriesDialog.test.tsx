import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));

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
};

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		const action = (args as { action: { action: string } }).action;
		if (action.action === "list_plans") return { type: "plans", value: [plan] };
		if (action.action === "list_stories") return { type: "stories", value: [story] };
		if (action.action === "plan_state") return { type: "plan_state", value: "active" };
		if (action.action === "transition")
			return { type: "story", value: { ...story, status: "in_progress", revision: 2 } };
		throw new Error(`unexpected action ${action.action}`);
	});
});

describe("StoriesDialog", () => {
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
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_plans") return { type: "plans", value: created ? [plan] : [] };
			if (action.action === "create_plan") {
				created = true;
				return { type: "plan", value: plan };
			}
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
			project: "/repo",
			action: { action: "create_plan", title: "Plan A", source: "plans/a.md" },
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
});
