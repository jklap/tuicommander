import { readFileSync } from "node:fs";
import { expect, it, vi } from "vitest";

const DIR = "browser-extension/design-mode";

// Catches: the dev-server list pushing pick cards below the fold, and a refresh re-collapsing a list the user opened.
it("dev_server_list_is_collapsed_by_default", async () => {
	vi.useFakeTimers();
	const html = readFileSync(`${DIR}/panel.html`, "utf8");
	document.body.innerHTML = html.slice(html.indexOf("<body>") + 6, html.indexOf("<script"));
	const servers = Array.from({ length: 9 }, (_, i) => ({ url: `http://localhost:${5000 + i}/`, source: "listener" }));
	const snapshot = {
		tabId: 1,
		repoPath: "/repo",
		connection: "Connected · sock",
		agents: [],
		servers: { servers, canStart: false },
		picks: [{ id: "p1", comment: "", raw: { tagName: "button", selector: "#send", textContent: "Send" } }],
	};
	const chrome = {
		runtime: {
			sendMessage: vi.fn(async () => ({ ok: true, result: snapshot })),
			onMessage: { addListener: () => {} },
		},
	};
	new Function("chrome", readFileSync(`${DIR}/panel.js`, "utf8"))(chrome);
	await vi.waitFor(() => expect(document.querySelectorAll("#cards .card")).toHaveLength(1));

	const list = document.getElementById("servers") as HTMLDetailsElement;
	expect(list.tagName).toBe("DETAILS");
	expect(list.open).toBe(false);
	expect(list.querySelector("summary")?.textContent).toBe("Dev servers (9)");
	expect(list.querySelectorAll("button")).toHaveLength(10); // 9 servers + refresh

	list.open = true;
	const calls = chrome.runtime.sendMessage.mock.calls.length;
	await vi.advanceTimersByTimeAsync(5000);
	expect(chrome.runtime.sendMessage.mock.calls.length).toBeGreaterThan(calls);
	expect(list.open).toBe(true);
	expect(list.querySelectorAll("button")).toHaveLength(10);
	vi.useRealTimers();
});
