// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { TasksDetail } from "../TasksDetail";
import { detail, provider, reference } from "./detailFixture";

const observe = vi.hoisted(() => vi.fn());
let followed = () => false;
let setFollowed: (value: boolean) => void;
vi.mock("../../../../services/tasksNotifications", () => ({
	tasksNotifications: {
		observe,
		followed: () => followed(),
		setFollowed: (_reference: unknown, value: boolean) => setFollowed(value),
	},
}));
afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

// Catches: Follow never reaches persistence, or accepted existing refreshes never feed comment comparison.
it("follow_toggle_and_existing_refresh_feed_authoritative_detail", async () => {
	const [get, set] = createSignal(false);
	followed = get;
	setFollowed = set;
	const value = detail();
	render(() => <TasksDetail provider={provider({ detail: async () => value })} reference={reference} />);
	fireEvent.click(await screen.findByRole("button", { name: "Follow task" }));
	expect(screen.getByRole("button", { name: "Unfollow task" })).toHaveAttribute("aria-pressed", "true");
	expect(observe).toHaveBeenLastCalledWith(value, true);
	observe.mockClear();
	fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
	await waitFor(() => expect(observe).toHaveBeenCalledWith(value, false));
	fireEvent.click(screen.getByRole("button", { name: "Unfollow task" }));
	expect(screen.getByRole("button", { name: "Follow task" })).toHaveAttribute("aria-pressed", "false");
});
