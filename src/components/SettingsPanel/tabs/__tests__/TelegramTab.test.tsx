import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TelegramTab } from "../TelegramTab";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../../../invoke", () => ({ invoke }));
const initial = {
	enabled: true,
	token_set: true,
	bot_alias: "test_bot",
	target_tuic_session: "peer-id",
	chats: ["123"],
	agents: [{ id: "peer-id", name: "Writer" }],
	connected: true,
	last_error: null,
	last_message_time: null,
};
beforeEach(() => {
	invoke.mockReset();
	invoke.mockImplementation(async (command: string) =>
		command === "telegram_settings" ? initial : { code: "ABC123" },
	);
});
afterEach(cleanup);
describe("Telegram setup", () => {
	// Catches: password is shown as plain text or retained in the form after sending.
	it("clears the password and sends only the replacement action", async () => {
		render(() => <TelegramTab />);
		const field = await screen.findByLabelText("Bot token");
		expect(field.getAttribute("type")).toBe("password");
		fireEvent.input(field, { target: { value: "123:fake-token" } });
		fireEvent.click(screen.getByText("Save and check bot"));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("telegram_setup", { change: { action: "token", token: "123:fake-token" } }),
		);
		expect((field as HTMLInputElement).value).toBe("");
		expect(screen.queryByText("123:fake-token")).toBeNull();
	});
	// Catches: setup invents a UUID/chat binding instead of submitting the explicit user choice.
	it("offers both pairing and explicit chat IDs with removal and live target selection", async () => {
		render(() => <TelegramTab />);
		await screen.findByLabelText("Authorized chats");
		fireEvent.click(screen.getByText("Link chat"));
		await screen.findByText(/Send ABC123/);
		await waitFor(() => expect(screen.getByText("Link chat").hasAttribute("disabled")).toBe(false));
		fireEvent.input(screen.getByLabelText("Authorized chats"), { target: { value: "456" } });
		fireEvent.click(screen.getByText("Add chat ID"));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("telegram_setup", { change: { action: "add_chat", chat_id: "456" } }),
		);
		await waitFor(() => expect(screen.getByText("Remove").hasAttribute("disabled")).toBe(false));
		fireEvent.click(screen.getByText("Remove"));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("telegram_setup", { change: { action: "remove_chat", chat_id: "123" } }),
		);
		expect(screen.getByRole("option", { name: "Writer" }).getAttribute("value")).toBe("peer-id");
	});
	// Catches: a failed getMe appears successful or drops the safe error category.
	it("shows typed setup errors", async () => {
		invoke.mockImplementation(async (command: string) => {
			if (command === "telegram_settings") return initial;
			throw "telegram_unauthorized";
		});
		render(() => <TelegramTab />);
		fireEvent.input(await screen.findByLabelText("Bot token"), { target: { value: "bad-token" } });
		fireEvent.click(screen.getByText("Save and check bot"));
		expect((await screen.findByRole("alert")).textContent).toContain("telegram_unauthorized");
	});
});
