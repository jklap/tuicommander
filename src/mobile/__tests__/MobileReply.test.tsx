import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";
import { HttpRpcError } from "../../transport";

const { rpc } = vi.hoisted(() => ({
	rpc: vi.fn(async (_command: string, _args: Record<string, unknown>) => ({ status: "acknowledged", submitted: true, acknowledged: true })),
}));
vi.mock("../../transport", async (importOriginal) => ({ ...(await importOriginal<typeof import("../../transport")>()), rpc }));
const { toastAdd } = vi.hoisted(() => ({ toastAdd: vi.fn() }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: toastAdd } }));

afterEach(() => {
	cleanup();
	rpc.mockClear();
	toastAdd.mockClear();
});

describe("mobile managed-agent reply", () => {
	it("sends the complete answer once after the user submits it", async () => {
		const { container } = render(() => (
			<CommandInput sessionId="question-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Please wait for me" } });
		expect(rpc).not.toHaveBeenCalled();
		await fireEvent.click(container.querySelector("button[type=button]")!);
		await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1));
		expect(rpc).toHaveBeenCalledWith("submit_agent_reply", {
			sessionId: "question-session",
			input: "Please wait for me",
		});
	});

	it("does not write any answer to a closed session", async () => {
		const { container } = render(() => (
			<CommandInput sessionId="closed-session" agentType="claude" awaitingInput={true} managedSession={true} sessionExists={false} />
		));
		await fireEvent.input(container.querySelector("textarea")!, { target: { value: "yes" } });
		await fireEvent.click(container.querySelector("button[type=button]")!);
		expect(rpc).not.toHaveBeenCalled();
	});

	it("keeps an answer editable when the session rejects it", async () => {
		rpc.mockImplementationOnce(async () => ({ status: "rejected", submitted: false, acknowledged: false }));
		const { container } = render(() => (
			<CommandInput sessionId="busy-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Wait for approval" } });
		await fireEvent.click(container.querySelector("button[type=button]")!);
		await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1));
		expect(input.value).toBe("Wait for approval");
	});

	it("explains a closed-session HTTP rejection without losing the answer", async () => {
		rpc.mockRejectedValueOnce(new HttpRpcError("submit_agent_reply", 404, '{"submitted":false,"reason":"session_not_found"}'));
		const { container } = render(() => (
			<CommandInput sessionId="closed-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Wait for me" } });
		await fireEvent.click(container.querySelector("button[type=button]")!);
		await waitFor(() => expect(toastAdd).toHaveBeenCalledWith("Reply not sent", "This session has ended", "error", true));
		expect(input.value).toBe("Wait for me");
	});

	it("explains a busy-agent HTTP rejection without losing the answer", async () => {
		rpc.mockRejectedValueOnce(new HttpRpcError("submit_agent_reply", 409, '{"submitted":false,"reason":"agent_not_ready"}'));
		const { container } = render(() => (
			<CommandInput sessionId="busy-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Wait for me" } });
		await fireEvent.click(container.querySelector("button[type=button]")!);
		await waitFor(() => expect(toastAdd).toHaveBeenCalledWith("Reply not sent", "The agent is busy. Check the session before retrying.", "error", true));
		expect(input.value).toBe("Wait for me");
	});

	it("does not submit the same answer twice while the first receipt is pending", async () => {
		let finish!: (receipt: { status: string; submitted: boolean; acknowledged: boolean }) => void;
		rpc.mockImplementationOnce(
			() => new Promise((resolve) => {
				finish = resolve;
			}),
		);
		const { container } = render(() => (
			<CommandInput sessionId="question-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		await fireEvent.input(container.querySelector("textarea")!, { target: { value: "yes" } });
		const send = container.querySelector("button[type=button]")!;
		await fireEvent.click(send);
		await fireEvent.click(send);
		expect(rpc).toHaveBeenCalledTimes(1);
		finish({ status: "acknowledged", submitted: true, acknowledged: true });
		await waitFor(() => expect(container.querySelector("textarea")!.value).toBe(""));
	});

	it("does not leak editing keys to a waiting managed agent", async () => {
		const { container } = render(() => (
			<CommandInput sessionId="question-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "draft" } });
		await fireEvent.keyDown(input, { key: "Tab" });
		await fireEvent.keyDown(input, { key: "Escape" });
		expect(rpc).not.toHaveBeenCalled();
	});

	it("keeps numbered choice prompts on the key input path", async () => {
		const { container } = render(() => (
			<CommandInput
				sessionId="choice-session"
				agentType="claude"
				awaitingInput={true}
				managedSession={true}
				choicePrompt={{
					title: "Choose one",
					options: [{ key: "1", label: "Approve", highlighted: false, destructive: false }],
				}}
			/>
		));
		await fireEvent.click(Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("Approve"))!);
		expect(rpc.mock.calls.some(([command]) => command === "submit_agent_reply")).toBe(false);
		expect(rpc.mock.calls.some(([command, args]) => command === "write_pty" && args.data === "1")).toBe(true);
	});
});
