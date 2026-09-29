import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";

const writes: { data: string; at: number }[] = [];
vi.mock("../../transport", () => ({
	rpc: vi.fn((_command: string, args: { data?: string }) => {
		if (args.data !== undefined) writes.push({ data: args.data, at: Date.now() });
		return Promise.resolve(undefined);
	}),
	HttpRpcError: class extends Error {},
}));

import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	writes.length = 0;
});

describe("mobile slash submission", () => {
	it.each([
		["codex", "/status", 180],
		["claude", "/help", 40],
	])("submits a typed %s slash command after the agent input gap", async (agentType, command, minGap) => {
		const { container } = render(() => <CommandInput sessionId="disposable" agentType={agentType} />);
		const input = container.querySelector("textarea")!;
		fireEvent.input(input, { target: { value: command } });
		fireEvent.click(container.querySelector("button.send")!);
		await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
		expect(writes.map((write) => write.data)).toEqual([command, "\r"]);
		expect(writes[1].at - writes[0].at).toBeGreaterThanOrEqual(minGap);
	});

	it.each([
		["codex", "/status", 180],
		["claude", "/help", 40],
	])("submits a %s slash menu pick once with its selected command", async (agentType, command, minGap) => {
		const { container } = render(() => (
			<CommandInput
				sessionId="disposable"
				agentType={agentType}
				slashItems={[{ command, description: "Show status", highlighted: true }]}
			/>
		));
		const input = container.querySelector("textarea")!;
		fireEvent.input(input, { target: { value: command.slice(0, 3) } });
		fireEvent.click(Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(command))!);
		fireEvent.click(container.querySelector("button.send")!);
		await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
		expect(writes.map((write) => write.data)).toEqual([command.slice(0, 3), command.slice(3) + " ", "\r"]);
		expect(writes[2].at - writes[1].at).toBeGreaterThanOrEqual(minGap);
	});

	it("submits a Claude command with arguments without losing the final word", async () => {
		const { container } = render(() => <CommandInput sessionId="disposable" agentType="claude" />);
		fireEvent.input(container.querySelector("textarea")!, { target: { value: "/model opus" } });
		fireEvent.click(container.querySelector("button.send")!);
		await waitFor(() => expect(writes.some((write) => write.data === "\r")).toBe(true));
		expect(writes.map((write) => write.data)).toEqual(["/model opus", "\r"]);
	});

	it("does not press Enter while the final mobile write is still in flight", async () => {
		let releaseWrite!: () => void;
		const inFlight = new Promise<void>((resolve) => { releaseWrite = resolve; });
		vi.mocked(rpc).mockImplementationOnce(async (_command, args) => {
			writes.push({ data: args?.data as string, at: Date.now() });
			await inFlight;
			return undefined as never;
		});
		const { container } = render(() => <CommandInput sessionId="disposable" agentType="codex" />);
		fireEvent.input(container.querySelector("textarea")!, { target: { value: "/status" } });
		fireEvent.click(container.querySelector("button.send")!);
		await new Promise((resolve) => setTimeout(resolve, 230));
		expect(writes.map((write) => write.data)).toEqual(["/status"]);
		releaseWrite();
		await waitFor(() => expect(writes.map((write) => write.data)).toEqual(["/status", "\r"]));
	});
});
