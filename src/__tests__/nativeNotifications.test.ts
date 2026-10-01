import { beforeEach, describe, expect, it, vi } from "vitest";

const { request, send, nativeInvoke, warn } = vi.hoisted(() => ({
	request: vi.fn(),
	send: vi.fn(),
	nativeInvoke: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
	requestPermission: request,
	sendNotification: send,
}));
vi.mock("../transport", () => ({ isTauri: () => true }));
vi.mock("../invoke", () => ({ invoke: nativeInvoke }));
vi.mock("../stores/appLogger", () => ({ appLogger: { warn } }));

describe("native notification delivery", () => {
	beforeEach(() => {
		vi.resetModules();
		vi.clearAllMocks();
		request.mockResolvedValue("granted");
		nativeInvoke.mockResolvedValue(undefined);
	});

	it("does not ask permission or notify while TUIC is focused", async () => {
		const focus = vi.spyOn(document, "hasFocus").mockReturnValue(true);
		try {
			const { showNativeNotice } = await import("../services/nativeNotifications");
			await showNativeNotice({
				title: "Agent needs input",
				body: "Deploy Agent",
				key: "question:term-1",
				target: { kind: "terminal", id: "term-1" },
			});
			expect(request).not.toHaveBeenCalled();
			expect(send).not.toHaveBeenCalled();
		} finally {
			focus.mockRestore();
		}
	});

	// Catches: a reminder for a background tab being swallowed by the focus guard.
	it("notifies while focused when the caller sets ignoreFocus", async () => {
		const focus = vi.spyOn(document, "hasFocus").mockReturnValue(true);
		try {
			const { showNativeNotice } = await import("../services/nativeNotifications");
			await showNativeNotice({
				title: "Agent needs input",
				body: "Deploy Agent",
				key: "reminder:question:term-1",
				target: { kind: "terminal", id: "term-1" },
				ignoreFocus: true,
			});
			expect(send).toHaveBeenCalledWith({ title: "Agent needs input", body: "Deploy Agent" });
		} finally {
			focus.mockRestore();
		}
	});

	it("does not notify if TUIC gains focus while permission is pending", async () => {
		let focused = false;
		const focus = vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
		let allowPermission: (value: string) => void = () => {};
		request.mockImplementation(() => new Promise<string>((resolve) => (allowPermission = resolve)));
		try {
			const { showNativeNotice } = await import("../services/nativeNotifications");
			const pending = showNativeNotice({
				title: "Agent needs input",
				body: "Deploy Agent",
				key: "question:term-1",
				target: { kind: "terminal", id: "term-1" },
			});
			focused = true;
			allowPermission("granted");
			await pending;
			expect(send).not.toHaveBeenCalled();
		} finally {
			focus.mockRestore();
		}
	});

	it("coalesces repeated identical notices but keeps a different outcome", async () => {
		const focus = vi.spyOn(document, "hasFocus").mockReturnValue(false);
		try {
			const { showNativeNotice } = await import("../services/nativeNotifications");
			const done = {
				title: "Repo · Progress done",
				body: "Build finished",
				key: "progress:/repo:done:Build finished",
				target: { kind: "progress" as const, project: "/repo" },
			};
			await Promise.all([showNativeNotice(done), showNativeNotice(done)]);
			await showNativeNotice({
				title: "Repo · Progress blocked",
				body: "Need approval",
				key: "progress:/repo:blocked:Need approval",
				target: { kind: "progress", project: "/repo" },
			});
			expect(send).toHaveBeenCalledTimes(2);
			expect(request).toHaveBeenCalledTimes(1);
		} finally {
			focus.mockRestore();
		}
	});

	it("asks once and logs a denied permission without sending", async () => {
		request.mockResolvedValue("denied");
		const focus = vi.spyOn(document, "hasFocus").mockReturnValue(false);
		try {
			const { showNativeNotice } = await import("../services/nativeNotifications");
			await showNativeNotice({
				title: "Agent needs input",
				body: "A",
				key: "question:a",
				target: { kind: "terminal", id: "a" },
			});
			await showNativeNotice({
				title: "Agent needs input",
				body: "B",
				key: "question:b",
				target: { kind: "terminal", id: "b" },
			});
			expect(request).toHaveBeenCalledTimes(1);
			expect(send).not.toHaveBeenCalled();
			expect(warn).toHaveBeenCalledWith("app", "Native notifications permission denied");
		} finally {
			focus.mockRestore();
		}
	});

	it("uses the macOS click-aware native sender with a terminal target", async () => {
		const focus = vi.spyOn(document, "hasFocus").mockReturnValue(false);
		const userAgent = vi
			.spyOn(navigator, "userAgent", "get")
			.mockReturnValue("Mozilla/5.0 (Macintosh; Intel Mac OS X)");
		try {
			const { showNativeNotice } = await import("../services/nativeNotifications");
			await showNativeNotice({
				title: "Agent needs input",
				body: "Deploy Agent",
				key: "question:deploy",
				target: { kind: "terminal", id: "deploy" },
			});
			expect(nativeInvoke).toHaveBeenCalledWith("show_native_notification", {
				title: "Agent needs input",
				body: "Deploy Agent",
				target: { kind: "terminal", id: "deploy" },
			});
			expect(send).not.toHaveBeenCalled();
		} finally {
			focus.mockRestore();
			userAgent.mockRestore();
		}
	});
});
