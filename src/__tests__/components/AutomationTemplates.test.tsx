import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { AutomationsDialog } from "../../components/AutomationsDialog/AutomationsDialog";
import { repositoriesStore } from "../../stores/repositories";
import { definition, fakeAdapter } from "../automationsFixtures";

beforeEach(() => {
	for (const path of Object.keys(repositoriesStore.state.repositories)) repositoriesStore.remove(path);
	repositoriesStore.add({ path: "/chosen", displayName: "Chosen" });
	repositoriesStore.add({ path: "/other", displayName: "Other" });
});

// Catches: applying a template saving/running without consent, dropping backend
// content or selected inputs, and displaying run controls for an unsaved draft.
it("applies a backend draft only after repository and config selection, then saves it paused", async () => {
	const saves: unknown[] = [];
	const runNow = vi.fn(async () => ({ status: "reserved" as const, reason: null }));
	const requests: unknown[] = [];
	render(() => (
		<AutomationsDialog
			onClose={() => {}}
			adapter={fakeAdapter({
				templates: async () => [{ id: "daily_review", name: "Daily change review" }],
				template: async (...args) => {
					requests.push(args);
					return {
						...definition,
						id: "backend-draft",
						name: "Daily change review",
						prompt: "Backend literal prompt",
						repository: args[1],
						run_config: args[2],
						timezone: "Europe/Madrid",
						cron: "0 9 * * *",
						enabled: false,
					};
				},
				save: async (value) => {
					saves.push(structuredClone(value));
					return value;
				},
				runNow,
			})}
		/>
	));
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	await screen.findByRole("option", { name: "Daily change review" });
	fireEvent.change(screen.getByLabelText("Template"), { target: { value: "daily_review" } });
	fireEvent.change(screen.getByLabelText("Repository"), { target: { value: "/chosen" } });
	fireEvent.input(screen.getByLabelText("Run config"), { target: { value: "my-config" } });
	fireEvent.click(screen.getByRole("button", { name: "Apply template" }));
	await waitFor(() => expect(screen.getByLabelText("Prompt")).toHaveValue("Backend literal prompt"));
	expect(requests).toEqual([["daily_review", "/chosen", "my-config", ""]]);
	expect(screen.getByLabelText("Timezone")).toHaveValue("Europe/Madrid");
	expect(saves).toHaveLength(0);
	expect(runNow).not.toHaveBeenCalled();
	expect(screen.queryByRole("button", { name: "Run now" })).toBeNull();
	await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(saves).toHaveLength(1));
	expect(saves[0]).toMatchObject({ repository: "/chosen", run_config: "my-config", enabled: false });
	await screen.findByRole("button", { name: "Resume" });
	expect(runNow).not.toHaveBeenCalled();
});

// Catches: catalogue errors silently looking like an empty list without recovery.
it("shows a failed template catalogue and allows an explicit retry", async () => {
	let fail = true;
	render(() => (
		<AutomationsDialog
			onClose={() => {}}
			adapter={fakeAdapter({
				templates: async () => {
					if (fail) throw new Error("Templates unavailable");
					return [{ id: "daily_review", name: "Daily change review" }];
				},
			})}
		/>
	));
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	await screen.findByText("Templates unavailable");
	fail = false;
	const fieldset = screen.getByText("Built-in templates").closest("fieldset");
	const retry = fieldset?.querySelector("button");
	if (!retry) throw new Error("Template retry missing");
	fireEvent.click(retry);
	await screen.findByRole("option", { name: "Daily change review" });
	expect(screen.queryByText("Templates unavailable")).toBeNull();
});

// Catches: Apply template being a dead, disabled button that never says what is missing.
it("explains why a selected template cannot be applied yet and requests nothing", async () => {
	const template = vi.fn();
	render(() => (
		<AutomationsDialog
			onClose={() => {}}
			adapter={fakeAdapter({ templates: async () => [{ id: "daily_review", name: "Daily change review" }], template })}
		/>
	));
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	await screen.findByRole("option", { name: "Daily change review" });
	fireEvent.change(screen.getByLabelText("Template"), { target: { value: "daily_review" } });
	const apply = screen.getByRole("button", { name: "Apply template" });
	expect(apply).not.toBeDisabled();
	fireEvent.click(apply);
	expect((await screen.findByRole("alert")).textContent).toContain("repository");
	expect(template).not.toHaveBeenCalled();
});

// Catches: the repository field staying free text instead of offering the known repositories.
it("offers the known repositories in the repository picker", async () => {
	render(() => <AutomationsDialog onClose={() => {}} adapter={fakeAdapter()} />);
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	const picker = await screen.findByLabelText("Repository");
	expect(picker.tagName).toBe("SELECT");
	expect([...picker.querySelectorAll("option")].map((o) => o.getAttribute("value"))).toEqual(
		expect.arrayContaining(["/chosen", "/other"]),
	);
});
