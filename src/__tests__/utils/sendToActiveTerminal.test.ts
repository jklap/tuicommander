import { describe, expect, it, vi } from "vitest";

const { mockTerminals, mockSendCommand, mockGetShellFamily, mockInvoke, mockAppLogger } = vi.hoisted(() => ({
	mockTerminals: {
		getAgentTypeForSession: vi.fn(() => null as string | null),
		getActive: vi.fn(),
	},
	mockSendCommand: vi.fn().mockResolvedValue(undefined),
	mockGetShellFamily: vi.fn().mockResolvedValue("posix"),
	mockInvoke: vi.fn().mockResolvedValue(undefined),
	mockAppLogger: { error: vi.fn() },
}));

vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminals }));
vi.mock("../../stores/appLogger", () => ({ appLogger: mockAppLogger }));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("../../utils/sendCommand", () => ({
	sendCommand: mockSendCommand,
	getShellFamily: mockGetShellFamily,
}));

import { sendTextToActiveTerminal, sendTextToSession } from "../../utils/sendToActiveTerminal";

describe("sendTextToSession", () => {
	it("looks up the agent type and shell family, then routes through sendCommand with a write_pty writer", async () => {
		mockTerminals.getAgentTypeForSession.mockReturnValue("claude");
		mockGetShellFamily.mockResolvedValue("posix");

		await sendTextToSession("s1", "ls -la", true);

		expect(mockTerminals.getAgentTypeForSession).toHaveBeenCalledWith("s1");
		expect(mockGetShellFamily).toHaveBeenCalledWith("s1");
		expect(mockSendCommand).toHaveBeenCalledWith(expect.any(Function), "ls -la", "claude", "posix", true);

		// The writer passed to sendCommand must invoke() write_pty for this session.
		const calls = mockSendCommand.mock.calls;
		const writer = calls[calls.length - 1][0] as (data: string) => Promise<void>;
		await writer("hello");
		expect(mockInvoke).toHaveBeenCalledWith("write_pty", { sessionId: "s1", data: "hello" });
	});

	it("defaults submit to true when omitted", async () => {
		await sendTextToSession("s2", "echo hi");
		expect(mockSendCommand).toHaveBeenCalledWith(expect.any(Function), "echo hi", expect.anything(), "posix", true);
	});
});

describe("sendTextToActiveTerminal", () => {
	it("is a no-op when there is no active terminal", async () => {
		mockTerminals.getActive.mockReturnValue(undefined);
		mockSendCommand.mockClear();

		await sendTextToActiveTerminal("hello");

		expect(mockSendCommand).not.toHaveBeenCalled();
	});

	it("is a no-op when the active terminal has no sessionId", async () => {
		mockTerminals.getActive.mockReturnValue({ sessionId: null, ref: { focus: vi.fn() } });
		mockSendCommand.mockClear();

		await sendTextToActiveTerminal("hello");

		expect(mockSendCommand).not.toHaveBeenCalled();
	});

	it("sends to the active session and refocuses its ref", async () => {
		const focus = vi.fn();
		mockTerminals.getActive.mockReturnValue({ sessionId: "s3", ref: { focus } });
		mockSendCommand.mockClear();
		mockSendCommand.mockResolvedValue(undefined);

		await sendTextToActiveTerminal("hello");

		expect(mockSendCommand).toHaveBeenCalledWith(expect.any(Function), "hello", expect.anything(), "posix", true);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		expect(focus).toHaveBeenCalled();
	});

	it("logs and still refocuses when sendCommand rejects", async () => {
		const focus = vi.fn();
		mockTerminals.getActive.mockReturnValue({ sessionId: "s4", ref: { focus } });
		mockSendCommand.mockRejectedValueOnce(new Error("boom"));

		await sendTextToActiveTerminal("hello");

		expect(mockAppLogger.error).toHaveBeenCalledWith("network", "Send to terminal failed: boom");
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		expect(focus).toHaveBeenCalled();
	});
});
