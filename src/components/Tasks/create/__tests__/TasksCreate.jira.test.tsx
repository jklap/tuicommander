// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { expect, it } from "vitest";
import { TasksCreate } from "../TasksCreate";
import { jiraFixtureProvider, recordedProjects } from "./jiraFixtureProvider";

const cases = (Object.keys(recordedProjects) as (keyof typeof recordedProjects)[]).flatMap((project) => {
	const fixture = recordedProjects[project];
	return Object.keys(fixture.fields).map((id) => ({ project, id }));
});

// Catches: a required field has no usable control, or form values serialize to invalid Jira shapes.
it.each(cases)("recorded_$project_$id_collects_valid_create_payload_at_jira_boundary", async ({ project, id }) => {
	const { provider, writes } = jiraFixtureProvider(project);
	const view = render(() => <TasksCreate provider={provider} onCreated={() => {}} />);
	const picker = await screen.findByLabelText("Type *");
	await screen.findByRole("option", { name: project === "LAN" ? "Story" : "Workstream" });
	fireEvent.change(picker, { target: { value: id } });
	await waitFor(() => expect(screen.getByLabelText("Assignee")).toBeTruthy());
	fireEvent.input(screen.getByLabelText("Title *"), { target: { value: "Review task" } });
	expect(screen.queryByLabelText("Project *")).toBeNull();
	expect(screen.queryByLabelText("Issue Type *")).toBeNull();
	if (project === "LAN") {
		const reporter = screen.getByLabelText("Reporter *");
		const hintId = reporter.getAttribute("aria-describedby");
		expect(
			hintId
				?.split(" ")
				.map((id) => document.getElementById(id)?.textContent)
				.join(" "),
		).toContain("Required: Reporter");
		fireEvent.input(reporter, { target: { value: "Reviewer" } });
		fireEvent.click(await screen.findByRole("button", { name: "Reviewer 2" }));
		fireEvent.input(screen.getByLabelText("Labels"), { target: { value: "new-label," } });
		expect(screen.getByLabelText("Labels")).toHaveValue("new-label,");
		fireEvent.input(screen.getByLabelText("Labels"), { target: { value: "new-label, second-label" } });
	}
	const parent = screen.queryByLabelText("Parent *");
	if (parent) fireEvent.input(parent, { target: { value: "LAN-123" } });
	fireEvent.input(screen.getByLabelText("Assignee"), { target: { value: "Reviewer" } });
	fireEvent.click(await screen.findByRole("button", { name: "Reviewer 4" }));
	const create = screen.getByRole("button", { name: "Create" });
	await waitFor(() => expect(create).not.toBeDisabled());
	fireEvent.click(create);
	await waitFor(() => expect(writes).toHaveLength(1));
	expect(writes[0]).toMatchObject({
		fields: {
			summary: "Review task",
			project: { id: project === "LAN" ? "10009" : "13130" },
			issuetype: { id },
			assignee: { accountId: "review-user-4" },
		},
	});
	if (project === "LAN")
		expect(writes[0]).toMatchObject({
			fields: { reporter: { accountId: "review-user-2" }, labels: ["new-label", "second-label"] },
		});
	if (parent) expect(writes[0]).toMatchObject({ fields: { parent: { key: "LAN-123" } } });
	expect(await screen.findByRole("alert")).toHaveTextContent("Jira request failed (403)");
	view.unmount();
});
