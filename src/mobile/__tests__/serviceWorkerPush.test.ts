import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { expect, it } from "vitest";

type Notice = {
	tag: string;
	body: string;
	data: { url: string };
	close: () => void;
};

it("keeps different session pushes actionable while replacing repeats of one session", async () => {
	const handlers = new Map<string, (event: unknown) => void>();
	const notices = new Map<string, Notice>();
	const opened: string[] = [];
	const worker = readFileSync("public/sw.js", "utf8");
	runInNewContext(worker, {
		self: {
			addEventListener: (type: string, handler: (event: unknown) => void) => handlers.set(type, handler),
			registration: {
				showNotification: async (_title: string, options: Notice) => {
					// A browser replaces a visible notification with the same tag.
					notices.set(options.tag, { ...options, close: () => undefined });
				},
			},
		},
		clients: {
			matchAll: async () => [],
			openWindow: async (url: string) => {
				opened.push(url);
			},
		},
	});

	async function dispatch(type: string, fields: object) {
		const pending: Promise<unknown>[] = [];
		const handler = handlers.get(type);
		expect(handler, `${type} listener`).toBeDefined();
		handler?.({ ...fields, waitUntil: (promise: Promise<unknown>) => pending.push(promise) });
		await Promise.all(pending);
	}
	async function push(url: string, body: string) {
		await dispatch("push", { data: { json: () => ({ title: "TUICommander", body, url }) } });
	}

	await push("/mobile/session/a", "A first question");
	await push("/mobile/session/b", "B question");
	expect(notices.size).toBe(2);

	await push("/mobile/session/a", "A new question");
	expect(notices.size).toBe(2);
	const surviving = [...notices.values()];
	expect(surviving.map((notice) => notice.body).sort()).toEqual(["A new question", "B question"]);
	await push("/mobile", "General alert");
	await push("/mobile", "Updated general alert");
	expect(notices.size).toBe(3);
	expect([...notices.values()].map((notice) => notice.body).sort()).toEqual([
		"A new question",
		"B question",
		"Updated general alert",
	]);
	await push("/mobile?repo=%2Frepo&session=chat-a", "First chat question");
	await push("/mobile?repo=%2Frepo&session=chat-b", "Second chat question");
	expect(notices.size).toBe(5);
	expect(notices.has("tuic-acp-chat-a")).toBe(true);
	expect(notices.has("tuic-acp-chat-b")).toBe(true);
	expect([...notices.values()].map((notice) => notice.body)).toEqual(
		expect.arrayContaining(["First chat question", "Second chat question"]),
	);

	for (const notice of surviving) {
		await dispatch("notificationclick", { notification: notice });
	}
	expect(opened.sort()).toEqual(["/mobile/session/a", "/mobile/session/b"]);
});
