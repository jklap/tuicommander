import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkflowDesigner } from "../../components/WorkflowDesigner/WorkflowDesigner";
import { invoke } from "../../invoke";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

const draft = {
	id: "flow-1", project: "/repo", name: "Story delivery", kind: "story",
	draftRevision: 1, latestPublishedRevision: 0, builtinKey: null,
	graph: { nodes: [
		{ id: "start", kind: { type: "start" } },
		{ id: "work", kind: { type: "agent", role: "implementer", capabilities: ["story_read", "story_report"], prompt_template: "Work on {{story.id}}" } },
		{ id: "end", kind: { type: "end" } },
	], edges: [
		{ from: "start", to: "work", outcome: null },
		{ from: "work", to: "end", outcome: null },
	] },
};

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (_command, args) => {
		const action = (args as { action: { action: string; graph?: typeof draft.graph } }).action;
		if (action.action === "list_drafts") return { type: "drafts", value: [draft] };
		if (action.action === "update_draft") return { type: "draft", value: { ...draft, draftRevision: 2, graph: action.graph } };
		if (action.action === "publish") return { type: "published", value: { ...draft, revision: 2 } };
		throw new Error(`unexpected action ${action.action}`);
	});
});

describe("WorkflowDesigner", () => {
	it("adds and connects a node with keyboard controls, then saves and publishes", async () => {
		render(() => <WorkflowDesigner project="/repo" />);
		await screen.findByRole("heading", { name: "Workflow designer" });
		await screen.findByRole("button", { name: "Node work" });
		fireEvent.click(screen.getByRole("button", { name: "Add Notify" }));
		const notify = await screen.findByRole("button", { name: /Node notify/ });
		fireEvent.click(screen.getByRole("button", { name: "Node start" }));
		fireEvent.change(screen.getByRole("combobox", { name: "Connect to" }), { target: { value: notify.getAttribute("data-node-id") } });
		fireEvent.click(screen.getByRole("button", { name: "Connect nodes" }));
		fireEvent.click(notify);
		fireEvent.change(screen.getByRole("combobox", { name: "Connect to" }), { target: { value: "work" } });
		fireEvent.click(screen.getByRole("button", { name: "Connect nodes" }));
		fireEvent.click(screen.getByRole("button", { name: "Save draft" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("workflow_definition_action", {
			project: "/repo", action: expect.objectContaining({
				action: "update_draft", id: "flow-1", expected_revision: 1,
				graph: expect.objectContaining({
					nodes: expect.arrayContaining([expect.objectContaining({ kind: { type: "notify" } })]),
					edges: expect.arrayContaining([expect.objectContaining({ from: "start", to: notify.getAttribute("data-node-id"), outcome: null }), expect.objectContaining({ from: notify.getAttribute("data-node-id"), to: "work", outcome: null })]),
				}),
			}),
		}));
		fireEvent.click(screen.getByRole("button", { name: "Publish" }));
		await waitFor(() => expect(invoke).toHaveBeenCalledWith("workflow_definition_action", {
			project: "/repo", action: { action: "publish", id: "flow-1", expected_revision: 2 },
		}));
	});
	it("keeps unsaved changes when a different draft is selected", async () => {
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (action.action === "list_drafts") return { type: "drafts", value: [draft, { ...draft, id: "flow-2", name: "Alternative" }] };
			throw new Error(`unexpected action ${action.action}`);
		});
		render(() => <WorkflowDesigner project="/repo" />);
		await screen.findByRole("button", { name: "Add Notify" });
		fireEvent.click(screen.getByRole("button", { name: "Add Notify" }));
		fireEvent.click(screen.getByRole("button", { name: /Alternative/ }));
		expect(screen.getByRole("button", { name: /Node notify/ })).toBeTruthy();
		expect(screen.getByRole("button", { name: /Story delivery/ }).getAttribute("aria-current")).toBe("true");
		expect(screen.getByRole("alert").textContent).toContain("Save the current draft before switching.");
	});
});
