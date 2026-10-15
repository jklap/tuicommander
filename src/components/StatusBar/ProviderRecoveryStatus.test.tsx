import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { rpc } from "../../transport";
import { ProviderRecoveryStatus, recoveryLabel } from "./ProviderRecoveryStatus";

const { send } = vi.hoisted(() => ({
	send: vi.fn(async (_write: (data: string) => Promise<void>, _text: string, _agent: string) => {}),
}));
vi.mock("../../utils/sendCommand", () => ({ sendCommand: send }));
vi.mock("../../transport", () => ({ rpc: vi.fn() }));

describe("provider recovery", () => {
	it("keeps an elapsed retry visible instead of claiming recovery succeeded", () => {
		expect(recoveryLabel({ phase: "waiting", attempt: 3, next_attempt_ms: 1000 }, 2500)).toContain("(0s)");
		expect(recoveryLabel({ phase: "retrying", attempt: 4, next_attempt_ms: null }, 2500)).toContain(
			"retrying · attempt 4",
		);
	});
	it("submits retry once to its own session without a second click racing", async () => {
		render(() => (
			<ProviderRecoveryStatus sessionId="ego-a" recovery={{ phase: "waiting", attempt: 1, next_attempt_ms: 5000 }} />
		));
		fireEvent.click(screen.getByRole("button", { name: "Force retry" }));
		fireEvent.click(screen.getByRole("button", { name: "Force retry" }));
		expect(send).toHaveBeenCalledTimes(1);
		expect(send).toHaveBeenCalledWith(expect.any(Function), "/retry", "ego");
		const writer = send.mock.calls[0][0] as unknown as (data: string) => Promise<void>;
		await writer("/retry");
		expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "ego-a", data: "/retry" });
	});
});
