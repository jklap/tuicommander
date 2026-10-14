// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { expect, it } from "vitest";
import { TasksCreate } from "../TasksCreate";
import { jiraFixtureProvider, WIDE_LAN_TYPE } from "./jiraFixtureProvider";

async function openForm() {
	const { provider, writes } = jiraFixtureProvider("LAN_WIDE");
	render(() => <TasksCreate provider={provider} onCreated={() => {}} />);
	const picker = await screen.findByLabelText("Type *");
	await screen.findByRole("option", { name: "Task" });
	fireEvent.change(picker, { target: { value: WIDE_LAN_TYPE } });
	await screen.findByLabelText("Assignee");
	fireEvent.input(screen.getByLabelText("Title *"), { target: { value: "Review task" } });
	return writes;
}

async function pick(label: string, name: string) {
	fireEvent.input(screen.getByLabelText(label), { target: { value: "Reviewer" } });
	fireEvent.click(await screen.findByRole("button", { name }));
}

// Catches: a single-user field (Assignee) takes a second pick and fails only at submit.
it("single_user_field_replaces_the_first_pick_with_the_second", async () => {
	const writes = await openForm();
	await pick("Assignee", "Reviewer 4");
	await pick("Assignee", "Reviewer 2");
	expect(screen.getAllByRole("button", { name: /^Remove / }).map((button) => button.textContent)).toEqual([
		"Remove Reviewer 2",
	]);
	await waitFor(() => expect(screen.getByRole("button", { name: "Create" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Create" }));
	await waitFor(() => expect(writes).toHaveLength(1));
	expect(writes[0]).toMatchObject({ fields: { assignee: { accountId: "review-user-2" } } });
	expect(await screen.findByRole("alert")).toHaveTextContent("Jira request failed (403)");
});

// Catches: the chosen user shows only as a Remove button, so the field does not display or announce its value.
it("chosen_user_is_shown_and_announced_as_the_field_value", async () => {
	await openForm();
	await pick("Assignee", "Reviewer 4");
	const field = screen.getByLabelText("Assignee");
	expect(field).toHaveAccessibleDescription(/Selected: Reviewer 4/);
});

// Catches: Reporter has a Jira default yet the form forces a pick, and empty optional fields go out as null.
it("reporter_with_a_jira_default_is_optional_and_empty_optional_fields_are_not_sent", async () => {
	const writes = await openForm();
	expect(screen.getByLabelText("Reporter")).toBeTruthy();
	await waitFor(() => expect(screen.getByRole("button", { name: "Create" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Create" }));
	await waitFor(() => expect(writes).toHaveLength(1));
	const sent = (writes[0] as { fields: Record<string, unknown> }).fields;
	expect(Object.keys(sent).sort()).toEqual(["description", "issuetype", "project", "summary"]);
});
