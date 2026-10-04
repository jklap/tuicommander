import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TelegramTab } from "../TelegramTab";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../../../invoke", () => ({ invoke }));
const initial = {
	enabled: true,
	token_set: true,
	bot_alias: "test_bot",
	registered_agent_name: "Writer",
	chats: ["123"],
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
	it("offers both pairing and explicit chat IDs with removal", async () => {
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
	});
	// Catches: Settings authorizes a selected peer instead of respecting MCP registration.
	it("shows registration read-only and toggles without writing a target", async () => {
		render(() => <TelegramTab />);
		await screen.findByText("registered agent: Writer");
		expect(screen.queryByRole("combobox")).toBeNull();
		fireEvent.click(screen.getByRole("checkbox"));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("telegram_setup", { change: { action: "configure", enabled: false } }),
		);
	});
	// Catches: an unregistered adapter silently appears to have an agent.
	it("shows the explicit unregistered status", async () => {
		invoke.mockResolvedValue({ ...initial, registered_agent_name: null });
		render(() => <TelegramTab />);
		await screen.findByText("nessun agent registrato");
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
