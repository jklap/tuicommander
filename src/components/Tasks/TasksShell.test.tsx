import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { workItemProviderRegistry } from "../../plugins/workItemProviderRegistry";
import { listDemo } from "./list/demo";
import { TasksShell } from "./TasksShell";

// happy-dom has no layout. Supply viewport geometry, keeping the real virtualizer.
beforeEach(() => {
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
});
afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

describe("Tasks shell", () => {
	// Catches: the shell mounting views without any provider and showing a broken list.
	it("explains that a provider plugin is needed when none is registered", () => {
		render(() => <TasksShell />);
		expect(screen.getByText(/Install a work-item provider plugin/)).toBeInTheDocument();
	});

	// Catches: a list row click that never reaches the detail view.
	it("opens the detail of a clicked list row for the selected provider", async () => {
		const registration = workItemProviderRegistry.register(listDemo(3));
		try {
			render(() => <TasksShell />);
			fireEvent.click(await screen.findByRole("button", { name: /Task 00002/ }));
			expect(await screen.findByRole("region", { name: "Task detail" })).toBeInTheDocument();
			await waitFor(() => expect(screen.getByText("DEMO-2")).toBeInTheDocument());
		} finally {
			registration.dispose();
		}
	});

	// Catches: the board and roadmap views not being reachable from the shell.
	it("switches to the board and the roadmap views", async () => {
		const registration = workItemProviderRegistry.register(listDemo(3));
		try {
			render(() => <TasksShell />);
			await screen.findByRole("button", { name: /Task 00001/ });
			fireEvent.click(screen.getByRole("tab", { name: "Board" }));
			expect(await screen.findByRole("region", { name: "Tasks board" })).toBeInTheDocument();
			fireEvent.click(screen.getByRole("tab", { name: "Roadmap" }));
			expect(await screen.findByRole("region", { name: "Tasks roadmap" })).toBeInTheDocument();
		} finally {
			registration.dispose();
		}
	});

	// Catches: New task never reaching the create form of the selected provider.
	it("opens the create form for the selected provider", async () => {
		const registration = workItemProviderRegistry.register(listDemo(1));
		try {
			render(() => <TasksShell />);
			fireEvent.click(await screen.findByRole("button", { name: "New task" }));
			expect(await screen.findByRole("form", { name: "Create task" })).toBeInTheDocument();
		} finally {
			registration.dispose();
		}
	});

	// Catches: below 700 px the detail has no way back to the list (the list is hidden by CSS).
	it("back_control_returns_from_the_detail_to_the_list", async () => {
		const registration = workItemProviderRegistry.register(listDemo(3));
		try {
			render(() => <TasksShell />);
			const body = await screen.findByLabelText("Tasks shell").then((shell) => shell.querySelector("[data-detail]"));
			expect(body).toHaveAttribute("data-detail", "closed");
			fireEvent.click(await screen.findByRole("button", { name: /Task 00002/ }));
			await screen.findByRole("region", { name: "Task detail" });
			expect(body).toHaveAttribute("data-detail", "open");
			// Hidden above 700 px by a container query, which happy-dom evaluates as plain display:none.
			fireEvent.click(screen.getByRole("button", { name: "Back to list", hidden: true }));
			expect(screen.queryByRole("region", { name: "Task detail" })).not.toBeInTheDocument();
			expect(body).toHaveAttribute("data-detail", "closed");
			expect(screen.getByRole("button", { name: /Task 00002/ })).toBeInTheDocument();
		} finally {
			registration.dispose();
		}
	});
});
