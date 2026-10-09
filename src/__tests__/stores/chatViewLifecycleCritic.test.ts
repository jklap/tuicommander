import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("../../invoke", () => transport);
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

import { acpTranscript } from "../../stores/acpTranscript";
import { chatViewKey, chatViewStore } from "../../stores/chatView";

describe("Chat view listener registration across CLI/Chat remount", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		chatViewStore.reset();
		acpTranscript.clear(chatViewKey("terminal"));
	});

	it("late cleanup from the previous mount must not stop the new mount following", async () => {
		let finishOldRegistration!: (dispose: () => void) => void;
		const callbacks: Array<(event: { payload: { session_id: string } }) => void> = [];
		transport.listen.mockImplementation((_name, callback) => {
			callbacks.push(callback);
			if (callbacks.length === 1) {
				return new Promise<() => void>((resolve) => {
					finishOldRegistration = resolve;
				});
			}
			return Promise.resolve(() => {});
		});
		transport.invoke.mockResolvedValue({
			epoch: 1,
			nextSeq: 1,
			reset: true,
			updates: [
				{ sessionUpdate: "agent_message_chunk", messageId: "reply", content: { type: "text", text: "latest reply" } },
			],
			unknownRows: 0,
			malformedRows: 0,
		});

		// The first component is removed while native listen registration is pending.
		const oldMount = chatViewStore.watch("terminal");
		// A new Chat component mounts before the old registration settles.
		const closeNew = await chatViewStore.watch("terminal");
		// TerminalChatView's disposed branch immediately runs this late disposer.
		finishOldRegistration(() => {});
		(await oldMount)();
		callbacks[1]({ payload: { session_id: "terminal" } });
		await chatViewStore.refresh("terminal");

		expect(acpTranscript.entries(chatViewKey("terminal"))).toHaveLength(1);
		closeNew();
	});
});
