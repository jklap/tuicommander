import { mkdirSync, mkdtempSync, rmSync, rmdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import { createServer, type ViteDevServer } from "vite";

let server: ViteDevServer | undefined;
let scratch: string | undefined;
let watchedHtml: string | undefined;
let toolingHtml: string | undefined;
let nestedSourceHtml: string | undefined;
let nestedPublicHtml: string | undefined;

afterEach(async () => {
	await server?.close();
	if (scratch) rmSync(scratch, { recursive: true, force: true });
	if (watchedHtml) rmSync(watchedHtml, { force: true });
	if (toolingHtml) rmSync(toolingHtml, { force: true });
	if (nestedSourceHtml) {
		rmSync(nestedSourceHtml, { force: true });
		rmdirSync(join(process.cwd(), "src", ".tmp"));
	}
	if (nestedPublicHtml) {
		rmSync(nestedPublicHtml, { force: true });
		rmdirSync(join(process.cwd(), "public", ".tmp"));
	}
});

it("watches frontend HTML without reloading for tooling HTML under .tmp or tools", async () => {
	mkdirSync(".tmp", { recursive: true });
	scratch = mkdtempSync(join(process.cwd(), ".tmp", "vite-watch-"));
	mkdirSync(join(process.cwd(), "src", ".tmp"));
	mkdirSync(join(process.cwd(), "public", ".tmp"));
	nestedSourceHtml = join(process.cwd(), "src", ".tmp", "vite-watch-control.html");
	nestedPublicHtml = join(process.cwd(), "public", ".tmp", "vite-watch-control.html");
	server = await createServer({
		configFile: "vite.config.ts",
		server: { port: 0, strictPort: false, ws: { port: 0 } },
		optimizeDeps: { noDiscovery: true },
	});
	const watcherReady = new Promise<void>((resolve) => server!.watcher.once("ready", resolve));
	await server.listen();
	await watcherReady;
	const send = vi.spyOn(server.environments.client.hot, "send");
	const events: string[] = [];
	server.watcher.on("all", (_event, path) => events.push(path));
	// Prove the isolated server really observes HTML before checking exclusion.
	watchedHtml = join(server.config.root, "src", "vite-watch-control.html");
	writeFileSync(watchedHtml, "<html></html>");
	await new Promise((resolve) => setTimeout(resolve, 700));
	expect(events).toContain(watchedHtml);
	expect(send.mock.calls.some(([message]) => JSON.stringify(message).includes('"type":"full-reload"'))).toBe(true);
	send.mockClear();
	const html = join(scratch, "index.html");
	writeFileSync(html, "<html></html>");
	await new Promise((resolve) => setTimeout(resolve, 700));
	rmSync(html);
	await new Promise((resolve) => setTimeout(resolve, 700));
	toolingHtml = join(server.config.root, "tools", "vite-watch-control.html");
	writeFileSync(toolingHtml, "<html></html>");
	writeFileSync(nestedSourceHtml, "<html></html>");
	writeFileSync(nestedPublicHtml, "<html></html>");
	await new Promise((resolve) => setTimeout(resolve, 700));
	expect(events).not.toContain(html);
	expect(events).not.toContain(toolingHtml);
	expect(events).not.toContain(nestedSourceHtml);
	expect(events).not.toContain(nestedPublicHtml);
	expect(send.mock.calls.filter(([message]) => JSON.stringify(message).includes('"type":"full-reload"'))).toEqual([]);
}, 30_000);
