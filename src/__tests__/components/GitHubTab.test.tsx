import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { appLogger } from "../../stores/appLogger";

// Shared, hoisted RPC stub + mutable account list so add/remove round-trip.
const h = vi.hoisted(() => {
	type Account = { id: string; host: { host: string }; login: string | null; kind: string };
	const accounts: Account[] = [];
	// repoPath -> resolution DTO returned by github_resolve_repo
	const resolutions: Record<string, unknown> = {};
	const repos: Record<string, { path: string; displayName: string }> = {};
	let circleCiStatus = { configured: false, source: "none" };
	const rpc = vi.fn((cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
		switch (cmd) {
			case "github_resolve_repos": {
				const map: Record<string, unknown> = {};
				for (const p of (args?.repoPaths as string[]) ?? []) {
					map[p] = resolutions[p] ?? { status: "unmonitored" };
				}
				return Promise.resolve(map);
			}
			case "github_bind_repo":
				resolutions[String(args?.repoPath)] = {
					status: "bound",
					account: { id: args?.accountId },
					owner: "octocat",
					repo: "hello",
				};
				return Promise.resolve(undefined);
			case "github_unbind_repo":
				resolutions[String(args?.repoPath)] = {
					status: "needs-bind",
					candidates: [
						{
							account_id: "github.com",
							host: { host: "github.com" },
							owner: "octocat",
							repo: "hello",
							remote_name: "origin",
						},
					],
				};
				return Promise.resolve(true);
			case "github_auth_status":
				return Promise.resolve({
					authenticated: false,
					login: null,
					avatar_url: null,
					source: "none",
					scopes: null,
				});
			case "github_diagnostics":
				return Promise.resolve({
					circuit_breaker_open: false,
					circuit_breaker_status: "OK",
					repos_not_found: [],
					repos_monitored: 0,
				});
			case "circleci_token_status":
				return Promise.resolve(circleCiStatus);
			case "github_list_accounts":
				return Promise.resolve([...accounts]);
			case "github_start_login":
				return Promise.resolve({
					device_code: "dev-code",
					user_code: "WDJB-MJHT",
					verification_uri: "https://github.com/login/device",
					expires_in: 900,
					interval: 5,
				});
			case "github_poll_login":
			case "github_poll_add_account":
				return Promise.resolve({ status: "pending" });
			case "account_secret_form":
				return Promise.resolve({ status: "stored", names: [] });

			case "github_remove_account": {
				const idx = accounts.findIndex((a) => a.id === args?.id);
				if (idx >= 0) accounts.splice(idx, 1);
				return Promise.resolve(undefined);
			}
			default:
				return Promise.resolve(undefined);
		}
	});
	const writeClipboard = vi.fn().mockResolvedValue(undefined);
	return {
		events: { handlers: {} as Record<string, (payload: unknown) => void> },
		rpc,
		defaultRpc: rpc.getMockImplementation()!,
		accounts,
		resolutions,
		repos,
		writeClipboard,
		get circleCiStatus() {
			return circleCiStatus;
		},
		set circleCiStatus(value) {
			circleCiStatus = value;
		},
	};
});

vi.mock("../../transport", () => ({
	rpc: h.rpc,
	isTauri: () => true,
	subscribeEvents: vi.fn(async (handlers) => {
		h.events.handlers = handlers;
		return () => {};
	}),
}));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		get state() {
			return { repositories: h.repos };
		},
	},
}));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: h.writeClipboard }));
vi.mock("../../stores/github", () => ({ githubStore: { setIssueFilter: vi.fn(), setPrHideDrafts: vi.fn() } }));
vi.mock("../../stores/settings", () => ({ settingsStore: { state: {} } }));
vi.mock("../../stores/repoDefaults", () => ({ repoDefaultsStore: { state: {} } }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn() }));
vi.mock("../../components/SettingsPanel/SettingFields", () => ({
	SettingSelect: () => null,
	SettingToggle: () => null,
}));

async function loadTab() {
	const mod = await import("../../components/SettingsPanel/tabs/GitHubTab");
	return mod.GitHubTab;
}

function resetState() {
	h.accounts.length = 0;
	for (const k of Object.keys(h.resolutions)) delete h.resolutions[k];
	for (const k of Object.keys(h.repos)) delete h.repos[k];
	h.circleCiStatus = { configured: false, source: "none" };
	h.rpc.mockReset();
	h.rpc.mockImplementation(h.defaultRpc);
	h.writeClipboard.mockClear().mockResolvedValue(undefined);
	vi.mocked(appLogger.warn).mockClear();
}

describe("GitHubTab — single-account collapse (006)", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	it("collapses the account manager + bindings to a single affordance when there is only the default account", async () => {
		const { findByText, queryByText } = render(() => <GitHubTab />);
		// Only the compact disclosure shows.
		expect(await findByText("Add another GitHub account")).toBeTruthy();
		// The manager body and bindings stay hidden in the 99% case.
		expect(queryByText("Additional GitHub Accounts")).toBeNull();
		expect(queryByText("Repository Bindings")).toBeNull();
		expect(queryByText("No additional accounts configured.")).toBeNull();
	});

	it("reveals the manager when the disclosure is clicked", async () => {
		const { findByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Add another GitHub account"));
		expect(await findByText("Additional GitHub Accounts")).toBeTruthy();
		expect(await findByText("No additional accounts configured.")).toBeTruthy();
	});
});

describe("GitHubTab — default credential recovery", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	function authenticated(source: "oauth" | "env") {
		const original = h.rpc.getMockImplementation()!;
		let connected = true;
		h.rpc.mockImplementation((cmd, args) => {
			if (cmd === "github_auth_status" && connected) {
				return Promise.resolve({
					authenticated: true,
					login: "octocat",
					avatar_url: null,
					source,
					scopes: "read:org, repo",
				});
			}
			if (cmd === "github_logout" || cmd === "github_disconnect") connected = false;
			return original(cmd, args);
		});
	}

	// Catches: a real authenticated OAuth response has no Logout/re-login path.
	it("logs out the default OAuth account and offers device-flow login again", async () => {
		authenticated("oauth");
		const { findByText, queryByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Logout"));
		expect(await findByText("Login with GitHub")).toBeTruthy();
		expect(queryByText("octocat")).toBeNull();
		fireEvent.click(await findByText("Login with GitHub"));
		expect(await findByText("WDJB-MJHT")).toBeTruthy();
	});

	// Catches: environment credentials leave no route back to device-flow login.
	it("disconnects an environment account and offers device-flow login", async () => {
		authenticated("env");
		const { findByText, queryByText } = render(() => <GitHubTab />);
		expect(await findByText("Switch to OAuth")).toBeTruthy();
		fireEvent.click(await findByText("Disconnect"));
		expect(await findByText("Login with GitHub")).toBeTruthy();
		expect(queryByText("octocat")).toBeNull();
	});

	// Catches: a uniquely resolved repository opens the account binding manager.
	it("keeps bindings hidden for a uniquely resolved default account repository", async () => {
		authenticated("oauth");
		h.repos["/work/proj"] = { path: "/work/proj", displayName: "proj" };
		h.resolutions["/work/proj"] = { status: "bound", account: { id: "github.com" }, owner: "octocat", repo: "hello" };
		const { findByText, queryByText } = render(() => <GitHubTab />);
		await findByText("Logout");
		expect(queryByText("Repository Bindings")).toBeNull();
		expect(await findByText("Add another GitHub account")).toBeTruthy();
	});
});

describe("GitHubTab — additional accounts", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	it("auto-shows the manager and lists existing accounts when one is configured", async () => {
		h.accounts.push({ id: "ghe.acme.com", host: { host: "ghe.acme.com" }, login: "octocat", kind: "ghe_pat" });
		const { findByText } = render(() => <GitHubTab />);
		// No disclosure click needed — a 2nd account means the manager is shown.
		expect(await findByText("octocat")).toBeTruthy();
		expect(await findByText(/ghe\.acme\.com/)).toBeTruthy();
	});

	it("labels a named github.com account as OAuth (device flow)", async () => {
		h.accounts.push({ id: "octocat2", host: { host: "github.com" }, login: "octocat2", kind: "github_com_oauth" });
		const { findByText } = render(() => <GitHubTab />);
		expect(await findByText("octocat2")).toBeTruthy();
		expect(await findByText(/github\.com · OAuth/)).toBeTruthy();
	});

	// Catches: Enterprise PAT entry being exposed to the Settings webview.
	it("launches Enterprise secret entry with metadata only", async () => {
		const { getByText, getByPlaceholderText, queryByPlaceholderText, findByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Add another GitHub account"));
		await findByText("No additional accounts configured.");
		expect(queryByPlaceholderText("Personal Access Token")).toBeNull();
		fireEvent.input(getByPlaceholderText("ghe.example.com"), { target: { value: "ghe.acme.com" } });
		fireEvent.click(getByText("Add account"));
		await waitFor(() =>
			expect(h.rpc).toHaveBeenCalledWith("account_secret_form", {
				request: {
					action: "add",
					account: {
						provider: "github",
						host: "ghe.acme.com",
						kind: "bearer",
						label: "ghe.acme.com",
						username: null,
					},
				},
			}),
		);
	});

	it("starts a device-flow login when adding a github.com account (003)", async () => {
		const { findByText, getByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Add another GitHub account"));
		fireEvent.click(await findByText("Add github.com account"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("github_start_login"));
		// The polling card shows the "add account" variant.
		expect(getByText(/Add another account — enter this code/)).toBeTruthy();
	});

	it("logs (not throws) when the device-flow auto-copy of the code is denied", async () => {
		const err = new DOMException("Write permission denied.", "NotAllowedError");
		h.writeClipboard.mockRejectedValueOnce(err);
		const { findByText, getByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Add another GitHub account"));
		fireEvent.click(await findByText("Add github.com account"));

		// Login proceeds (code card still renders) despite the clipboard failure.
		await waitFor(() => expect(getByText(/Add another account — enter this code/)).toBeTruthy());
		await waitFor(() => expect(appLogger.warn).toHaveBeenCalledWith("github", "Clipboard auto-copy failed", err));
	});

	it("copyCode leaves the button unchanged (no 'Copied!') when the clipboard write fails", async () => {
		h.writeClipboard.mockRejectedValue(new DOMException("Write permission denied.", "NotAllowedError"));
		const { findByText, getByText, queryByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Add another GitHub account"));
		fireEvent.click(await findByText("Add github.com account"));
		await findByText(/Add another account — enter this code/);

		fireEvent.click(getByText("Copy code"));

		// Give the rejected promise a tick to settle, then confirm no false "Copied!" state.
		await waitFor(() => expect(h.writeClipboard).toHaveBeenCalledWith("WDJB-MJHT"));
		await new Promise((r) => setTimeout(r, 0));
		expect(queryByText("Copied!")).toBeNull();
	});

	it("removes an account via github_remove_account(id) and re-collapses", async () => {
		h.accounts.push({ id: "ghe.acme.com", host: { host: "ghe.acme.com" }, login: "octocat", kind: "ghe_pat" });
		const { findByText, getByText } = render(() => <GitHubTab />);
		await findByText("octocat");

		fireEvent.click(getByText("Remove"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("github_remove_account", { id: "ghe.acme.com" }));
		// Back to the single-account collapsed state.
		expect(await findByText("Add another GitHub account")).toBeTruthy();
	});
});

describe("GitHubTab — Repository bindings", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	it("binds an ambiguous repo to the chosen candidate via github_bind_repo", async () => {
		h.repos["/work/proj"] = { path: "/work/proj", displayName: "proj" };
		h.resolutions["/work/proj"] = {
			status: "needs-bind",
			candidates: [
				{
					account_id: "github.com",
					host: { host: "github.com" },
					owner: "octocat",
					repo: "hello",
					remote_name: "origin",
				},
				{
					account_id: "github.com",
					host: { host: "github.com" },
					owner: "upstream",
					repo: "hello",
					remote_name: "upstream",
				},
			],
		};
		const { findByText, getByText } = render(() => <GitHubTab />);
		// An ambiguous repo forces the bindings section open (needs attention).
		expect(await findByText("proj")).toBeTruthy();

		fireEvent.click(getByText("Bind"));
		await waitFor(() =>
			expect(h.rpc).toHaveBeenCalledWith("github_bind_repo", {
				repoPath: "/work/proj",
				accountId: "github.com",
				remoteName: "origin",
			}),
		);
	});

	it("unbinds a bound repo via github_unbind_repo when the manager is shown", async () => {
		// A 2nd account keeps the bindings section visible (not the collapsed 99% case).
		h.accounts.push({ id: "octocat2", host: { host: "github.com" }, login: "octocat2", kind: "github_com_oauth" });
		h.repos["/work/proj"] = { path: "/work/proj", displayName: "proj" };
		h.resolutions["/work/proj"] = {
			status: "bound",
			account: { id: "github.com", host: { host: "github.com" }, login: "octocat", kind: "github_com_oauth" },
			owner: "octocat",
			repo: "hello",
		};
		const { findByText, getByText } = render(() => <GitHubTab />);
		expect(await findByText("proj")).toBeTruthy();

		fireEvent.click(getByText("Unbind"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("github_unbind_repo", { repoPath: "/work/proj" }));
	});
});

describe("GitHubTab — CircleCI token", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	// Catches: a newly merged credential owner staying stale in the open Settings page.
	it("refreshes CircleCI status after an external settings write without saving a token draft", async () => {
		const { findByText, getByPlaceholderText } = render(() => <GitHubTab />);
		await findByText("Not configured");
		const input = getByPlaceholderText("CircleCI read-only token") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "unfinished token draft" } });
		h.circleCiStatus = { configured: true, source: "vault" };
		h.events.handlers["settings-changed"]({ section: "circleci", generation: 1, persisted: true, applied: true });
		expect(await findByText("Configured (vault)")).toBeTruthy();
		expect(input.value).toBe("unfinished token draft");
		expect(h.rpc).not.toHaveBeenCalledWith("circleci_set_token", expect.anything());
	});

	it("shows configured status and removes a stored token explicitly", async () => {
		h.circleCiStatus = { configured: true, source: "vault" };
		const { findByText, getByText } = render(() => <GitHubTab />);
		expect(await findByText("Configured (vault)")).toBeTruthy();
		fireEvent.click(getByText("Remove token"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("circleci_delete_token"));
	});

	it("saves a token through the CircleCI command and clears the field", async () => {
		const { findByText, getByPlaceholderText, getByText } = render(() => <GitHubTab />);
		await findByText("CircleCI");
		const input = getByPlaceholderText("CircleCI read-only token") as HTMLInputElement;
		fireEvent.input(input, { target: { value: "  circle-token  " } });
		fireEvent.click(getByText("Save token"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("circleci_set_token", { token: "circle-token" }));
		await waitFor(() => expect(input.value).toBe(""));
	});

	it("does not save an empty CircleCI token", async () => {
		const { findByText, getByText } = render(() => <GitHubTab />);
		await findByText("CircleCI");
		fireEvent.click(getByText("Save token"));
		expect(await findByText("Enter a CircleCI token before saving")).toBeTruthy();
		expect(h.rpc).not.toHaveBeenCalledWith("circleci_set_token", expect.anything());
	});
});
