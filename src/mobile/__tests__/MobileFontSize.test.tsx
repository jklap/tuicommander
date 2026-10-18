// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { invoke, subscribePty } = vi.hoisted(() => ({ invoke: vi.fn(), subscribePty: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke, listen: vi.fn(async () => () => {}) }));
vi.mock("../../transport", () => ({ subscribePty }));
vi.mock("../useMobileVoice", () => ({ useMobileVoice: () => ({ available: () => false }) }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() } }));

beforeEach(() => {
	vi.resetModules();
	localStorage.clear();
	invoke.mockResolvedValue(undefined);
	subscribePty.mockResolvedValue(() => {});
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({ ok: true, json: async () => ({ lines: ["CLI answer"], total_lines: 1, offset: 0 }) })),
	);
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

// Catches: independent font preferences are lost on reload or one view changes the other.
it("restores independent Terminal and Chat sizes without resizing the composer", async () => {
	const { SettingsScreen } = await import("../screens/SettingsScreen");
	const first = render(() => <SettingsScreen isConnected />);
	fireEvent.click(screen.getByRole("button", { name: "Larger Terminal font" }));
	fireEvent.click(screen.getByRole("button", { name: "Larger Terminal font" }));
	for (let i = 0; i < 4; i++) fireEvent.click(screen.getByRole("button", { name: "Larger Chat font" }));
	expect(screen.getByRole("status", { name: "Terminal font size" }).textContent).toBe("16 px");
	expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("18 px");
	expect(localStorage.getItem("tuic-mobile-terminal-font-size")).toBe("16");
	expect(localStorage.getItem("tuic-mobile-chat-font-size")).toBe("18");
	first.unmount();
	vi.resetModules();
	const { OutputView } = await import("../components/OutputView");
	const { CommandInput } = await import("../components/CommandInput");
	const { useSessionChatView } = await import("../components/useSessionChatView");
	invoke.mockResolvedValue({
		epoch: 1,
		nextSeq: 1,
		reset: true,
		updates: [
			{ sessionUpdate: "agent_message_chunk", messageId: "answer", content: { type: "text", text: "Chat answer" } },
		],
	});
	const view = render(() => {
		const chat = useSessionChatView({
			sessionId: () => "font-test",
			agentType: () => "claude",
			sessionExists: () => true,
			subscribeWake: () => () => {},
		});
		return (
			<>
				<OutputView sessionId="font-test" />
				<CommandInput sessionId="font-test" />
				<chat.Toggle />
				<chat.View />
			</>
		);
	});
	await screen.findByText("CLI answer");
	expect(view.container.querySelector("pre")?.style.fontSize).toBe("16px");
	expect(view.container.querySelector("textarea")?.style.getPropertyValue("--mobile-font-size")).toBe("");
	fireEvent.click(screen.getByRole("button", { name: "Chat" }));
	await screen.findByText("Chat answer");
	const transcript = view.container.querySelector("[data-mobile-reading]") as HTMLElement;
	expect(transcript.style.getPropertyValue("--font-base")).toBe("18px");
});

// Catches: invalid storage or repeated taps break bounds, or one control moves the other.
it("restores existing defaults for invalid values and bounds each control independently", async () => {
	localStorage.setItem("tuic-mobile-terminal-font-size", "NaN");
	localStorage.setItem("tuic-mobile-chat-font-size", "999");
	const { SettingsScreen } = await import("../screens/SettingsScreen");
	render(() => <SettingsScreen isConnected />);
	expect(screen.getByRole("status", { name: "Terminal font size" }).textContent).toBe("14 px");
	expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("14 px");
	for (let i = 0; i < 20; i++) fireEvent.click(screen.getByRole("button", { name: "Larger Terminal font" }));
	expect(screen.getByRole("status", { name: "Terminal font size" }).textContent).toBe("24 px");
	expect((screen.getByRole("button", { name: "Larger Terminal font" }) as HTMLButtonElement).disabled).toBe(true);
	expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("14 px");
	for (let i = 0; i < 20; i++) fireEvent.click(screen.getByRole("button", { name: "Smaller Chat font" }));
	await waitFor(() => expect(screen.getByRole("status", { name: "Chat font size" }).textContent).toBe("10 px"));
	expect((screen.getByRole("button", { name: "Smaller Chat font" }) as HTMLButtonElement).disabled).toBe(true);
	expect(screen.getByRole("status", { name: "Terminal font size" }).textContent).toBe("24 px");
});
