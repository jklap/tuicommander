import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
	rpc: vi.fn(),
	toastAdd: vi.fn(),
	add: vi.fn(() => "term-1"),
	update: vi.fn(),
	setActive: vi.fn(),
	assign: vi.fn(),
}));

vi.mock("../../transport", () => ({ rpc: mocks.rpc }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: mocks.toastAdd } }));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		add: mocks.add,
		update: mocks.update,
		setActive: mocks.setActive,
		get: () => undefined,
	},
}));
vi.mock("../../stores/settings", () => ({ settingsStore: { state: { defaultFontSize: 13 } } }));
vi.mock("../../utils/paneTabAssign", () => ({ assignTabToActiveGroup: mocks.assign }));

const ROOT = "/Users/me/Library/Application Support/com.tuic.commander/attachments";
const def = { config_dir: null, config_file: "/Users/me/.claude.json", trusted: false };
const priv = {
	config_dir: "/Users/me/.claude-private",
	config_file: "/Users/me/.claude-private/.claude.json",
	trusted: false,
};

async function load() {
	vi.resetModules();
	return import("../../utils/attachmentTrust");
}

beforeEach(() => {
	// The focus call runs on the next frame; run it inline so no timer outlives the test.
	vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
		callback(0);
		return 0;
	});
	for (const mock of Object.values(mocks)) mock.mockClear();
	mocks.add.mockReturnValue("term-1");
});

describe("attachmentTrustCommand", () => {
	// Catches: the default profile gets a CLAUDE_CONFIG_DIR override it does not need.
	it("types plain claude for the default profile", async () => {
		const { attachmentTrustCommand } = await load();
		expect(attachmentTrustCommand(def)).toBe("claude");
	});

	// Catches: a non-default profile's trust is granted in the wrong profile.
	it("scopes a non-default profile with CLAUDE_CONFIG_DIR", async () => {
		const { attachmentTrustCommand } = await load();
		const command = attachmentTrustCommand(priv);
		expect(command).toContain("CLAUDE_CONFIG_DIR=");
		expect(command).toContain("/Users/me/.claude-private");
		expect(command.endsWith("claude")).toBe(true);
	});
});

describe("warnIfAttachmentsUntrusted", () => {
	// Catches: a nag for users whose profiles already trust the folder, or who have no claude.
	it("stays quiet when every profile is trusted or there are none", async () => {
		const { warnIfAttachmentsUntrusted } = await load();
		mocks.rpc.mockResolvedValueOnce({ root: ROOT, profiles: [{ ...def, trusted: true }] });
		await warnIfAttachmentsUntrusted();
		mocks.rpc.mockResolvedValueOnce({ root: ROOT, profiles: [] });
		await warnIfAttachmentsUntrusted();
		expect(mocks.toastAdd).not.toHaveBeenCalled();
	});

	// Catches: more than one toast per launch, or a button that does not open the trust terminal.
	it("shows one toast per launch whose button opens the trust terminal", async () => {
		const { warnIfAttachmentsUntrusted } = await load();
		mocks.rpc.mockResolvedValue({ root: ROOT, profiles: [{ ...def, trusted: true }, priv] });
		await warnIfAttachmentsUntrusted();
		await warnIfAttachmentsUntrusted();
		expect(mocks.toastAdd).toHaveBeenCalledTimes(1);
		const action = mocks.toastAdd.mock.calls[0][4] as { label: string; onClick: () => void };
		expect(action.label).toBe("Trust now");
		mocks.rpc.mockImplementation(async (cmd: string) => (cmd === "can_spawn_session" ? true : undefined));
		action.onClick();
		await vi.waitFor(() => expect(mocks.update).toHaveBeenCalled());
		expect(mocks.update.mock.calls[0][1].pendingInitCommand).toContain("CLAUDE_CONFIG_DIR=");
	});

	// Catches: a failing status call surfacing as an unhandled rejection at startup.
	it("swallows a failed status call", async () => {
		const { warnIfAttachmentsUntrusted } = await load();
		mocks.rpc.mockRejectedValueOnce(new Error("down"));
		await expect(warnIfAttachmentsUntrusted()).resolves.toBeUndefined();
		expect(mocks.toastAdd).not.toHaveBeenCalled();
	});
});

describe("openAttachmentTrustTerminal", () => {
	// Catches: the terminal opens in a repo/branch instead of the attachments root, or never runs claude.
	it("opens an unattached terminal in the root and queues the command", async () => {
		const { openAttachmentTrustTerminal } = await load();
		mocks.rpc.mockResolvedValueOnce(true);
		await openAttachmentTrustTerminal({ root: ROOT, profiles: [def] }, def);
		expect(mocks.add).toHaveBeenCalledWith(expect.objectContaining({ cwd: ROOT, sessionId: null }));
		expect(mocks.update).toHaveBeenCalledWith("term-1", { pendingInitCommand: "claude" });
		expect(mocks.assign).toHaveBeenCalledWith("term-1", "terminal");
		expect(mocks.setActive).toHaveBeenCalledWith("term-1");
	});

	// Catches: opening past the 50-session cap.
	it("does nothing when no session can be spawned", async () => {
		const { openAttachmentTrustTerminal } = await load();
		mocks.rpc.mockResolvedValueOnce(false);
		await openAttachmentTrustTerminal({ root: ROOT, profiles: [def] }, def);
		expect(mocks.add).not.toHaveBeenCalled();
	});
});
