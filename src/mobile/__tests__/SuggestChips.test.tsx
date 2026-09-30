import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AGENT_ENTER_GAP_MS } from "../../utils/sendCommand";
import { SuggestChips } from "../components/SuggestChips";

vi.mock("../../transport", () => ({
	rpc: vi.fn().mockResolvedValue(undefined),
}));

import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

describe("SuggestChips", () => {
	it("renders a chip for each item", () => {
		const { container } = render(() => <SuggestChips sessionId="s1" items={["Run tests", "Review diff", "Deploy"]} />);
		const buttons = container.querySelectorAll("button");
		expect(buttons.length).toBe(3);
		expect(buttons[0].textContent).toBe("Run tests");
		expect(buttons[1].textContent).toBe("Review diff");
		expect(buttons[2].textContent).toBe("Deploy");
	});

	it("sends command via sendCommand (Ctrl-U+text then Enter) on click", async () => {
		// agentType forces the Ctrl-U-prefix branch regardless of host platform
		// detection in the test environment (sendCommand skips Ctrl-U on native
		// Windows shells only when no agent is detected).
		const { container } = render(() => <SuggestChips sessionId="s1" items={["Run tests"]} agentType="claude" />);
		const button = container.querySelector("button")!;
		await fireEvent.click(button);
		// Wait past two AGENT_ENTER_GAP_MS: with an agent attached sendCommand
		// separates Ctrl-U from the text, and the text from the Enter, by a real
		// elapsed gap each so the PTY cannot coalesce them into one read().
		await new Promise((r) => setTimeout(r, 2 * AGENT_ENTER_GAP_MS + 20));
		expect(rpc).toHaveBeenCalledTimes(3);
		expect(rpc).toHaveBeenNthCalledWith(1, "write_pty", { sessionId: "s1", data: "\x15" });
		expect(rpc).toHaveBeenNthCalledWith(2, "write_pty", { sessionId: "s1", data: "Run tests" });
		expect(rpc).toHaveBeenNthCalledWith(3, "write_pty", { sessionId: "s1", data: "\r" });
	});

	it("renders nothing when items is empty", () => {
		const { container } = render(() => <SuggestChips sessionId="s1" items={[]} />);
		const buttons = container.querySelectorAll("button");
		expect(buttons.length).toBe(0);
	});
});
