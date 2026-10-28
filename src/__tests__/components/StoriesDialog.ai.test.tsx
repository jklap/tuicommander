import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

const draft = {
	title: "Search saved work",
	storyType: "feature",
	priority: 2,
	problem: "Saved work is hard to find.",
	criteria: ["A title query finds matching work", "[MANUAL] Review keyboard navigation"],
};
let prepare: () => Promise<string>;
let generate: () => Promise<unknown>;
let cancel: () => Promise<unknown>;
const actions = () =>
	vi
		.mocked(invoke)
		.mock.calls.filter(([command]) => command === "story_action_command")
		.map(([, args]) => (args as { action: { action: string; input?: unknown } }).action);

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	prepare = async () => "fill-1";
	generate = async () => draft;
	cancel = async () => true;
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		if (command === "acp_one_shot_prepare") return prepare();
		if (command === "acp_one_shot_cancel") return cancel();
		if (command === "story_generate_draft") return generate();
		const action = (args as { action: { action: string } }).action;
		if (action.action === "list_plans")
			return { type: "plans", value: [{ id: "p1", title: "Plan", project: "/repo", source: "plan.md" }] };
		if (action.action === "plan_view")
			return { type: "plan_view", value: { stories: [], state: "draft", wontFixCount: 0, allCancelled: false } };
		if (action.action === "create_story") return { type: "story", value: { id: "s1" } };
		throw new Error(`Unexpected command ${command} ${action.action}`);
	});
});

async function openForm() {
	render(() => <StoriesDialog project="/repo" onClose={() => {}} />);
	const open = await screen.findByRole("button", { name: "New story" });
	await waitFor(() => expect((open as HTMLButtonElement).disabled).toBe(false));
	fireEvent.click(open);
	fireEvent.input(screen.getByLabelText("Request"), { target: { value: "Let me search saved work" } });
}

describe("AI story draft review boundary", () => {
	it("never saves generated fields until the user reviews and explicitly creates the story", async () => {
		// Catches: a successful AI fill bypasses review and calls create_story itself.
		await openForm();
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await waitFor(() => expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("Search saved work"));
		expect(actions().some((action) => action.action === "create_story")).toBe(false);
		expect(invoke).toHaveBeenCalledWith("story_generate_draft", {
			project: "/repo",
			request: "Let me search saved work",
			requestId: "fill-1",
		});
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Reviewed search" } });
		fireEvent.submit(screen.getByRole("button", { name: "Create story" }).closest("form")!);
		await waitFor(() =>
			expect(actions()).toContainEqual(
				expect.objectContaining({
					action: "create_story",
					problem: draft.problem,
					input: expect.objectContaining({
						title: "Reviewed search",
						criteria: draft.criteria,
						priority: 2,
						storyType: "feature",
					}),
				}),
			),
		);
	});

	it("cancels owned model work and rejects a late reply without clearing the request", async () => {
		// Catches: cancelled generation still overwrites the editable draft.
		let finish!: (value: unknown) => void;
		generate = () =>
			new Promise((resolve) => {
				finish = resolve;
			});
		await openForm();
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("story_generate_draft", expect.anything()));
		expect(screen.getByRole("status").textContent).toContain("Filling");
		fireEvent.click(screen.getByRole("button", { name: "Cancel fill" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("acp_one_shot_cancel", { requestId: "fill-1" }));
		finish(draft);
		await screen.findByText("Fill cancelled. Your request and form are kept.");
		expect((screen.getByLabelText("Request") as HTMLTextAreaElement).value).toBe("Let me search saved work");
		expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("");
		expect(actions().some((action) => action.action === "create_story")).toBe(false);
	});

	it("revokes a late reservation when cancelled before model launch", async () => {
		// Catches: Cancel while prepare is pending still launching model work when the handle arrives.
		let reserve!: (id: string) => void;
		prepare = () =>
			new Promise((resolve) => {
				reserve = resolve;
			});
		await openForm();
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		fireEvent.click(screen.getByRole("button", { name: "Cancel fill" }));
		await screen.findByText("Fill cancelled. Your request and form are kept.");
		reserve("late-handle");
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("acp_one_shot_cancel", { requestId: "late-handle" }));
		expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "story_generate_draft")).toBe(false);
	});

	it("keeps request and manually entered fields usable when generation fails", async () => {
		// Catches: malformed/model failure destroys user input or leaves the form locked.
		generate = async () => {
			throw new Error("Invalid story draft reply");
		};
		await openForm();
		fireEvent.input(screen.getByLabelText("Title"), { target: { value: "My title" } });
		fireEvent.click(screen.getByRole("button", { name: "Fill with AI" }));
		await screen.findByText("Invalid story draft reply");
		expect((screen.getByLabelText("Request") as HTMLTextAreaElement).value).toBe("Let me search saved work");
		expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("My title");
		expect((screen.getByRole("button", { name: "Fill with AI" }) as HTMLButtonElement).disabled).toBe(false);
	});
});
