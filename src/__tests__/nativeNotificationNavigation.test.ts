import { describe, expect, it, vi } from "vitest";

const terminal = vi.fn();
const progress = vi.fn();
const listen = vi.fn();

vi.mock("../utils/navigateToTerminal", () => ({ navigateToTerminal: terminal }));
vi.mock("../stores/progress", () => ({ progressStore: { open: progress } }));
vi.mock("../invoke", () => ({ listen }));

describe("native notification click", () => {
	it("opens the named terminal after a question", async () => {
		const { navigateFromNativeNotice } = await import("../services/nativeNotificationNavigation");
		navigateFromNativeNotice({ kind: "terminal", id: "question-terminal" });
		expect(terminal).toHaveBeenCalledWith("question-terminal");
		expect(progress).not.toHaveBeenCalled();
	});

	it("opens the matching project and terminal scope after a progress outcome", async () => {
		const { navigateFromNativeNotice } = await import("../services/nativeNotificationNavigation");
		navigateFromNativeNotice({ kind: "progress", project: "/other-project", ptyId: "agent-2" });
		expect(progress).toHaveBeenCalledWith("/other-project", "agent-2");
		expect(terminal).toHaveBeenCalledTimes(0);
	});

	it("routes a native click event to its recorded target", async () => {
		listen.mockImplementation((_name, handler) => {
			handler({ payload: { kind: "terminal", id: "from-notification" } });
			return Promise.resolve(() => {});
		});
		const { listenForNativeNoticeClicks } = await import("../services/nativeNotificationNavigation");
		await listenForNativeNoticeClicks();
		expect(listen).toHaveBeenCalledWith("native-notification-click", expect.any(Function));
		expect(terminal).toHaveBeenCalledWith("from-notification");
	});
});
