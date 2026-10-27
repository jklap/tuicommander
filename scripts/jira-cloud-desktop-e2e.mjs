#!/usr/bin/env node
/** Real installed Jira provider, native settings UI and authorized scratch-site writes. */
import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const [instance, accountId, projectId, issueType, output, consent, ...extra] = process.argv.slice(2);
if (
	!/^agent-test-[a-z0-9-]+$/.test(instance ?? "") ||
	!accountId ||
	!/^\d+$/.test(projectId ?? "") ||
	!/^\d+$/.test(issueType ?? "") ||
	!output ||
	consent !== "--write-straus-team-only" ||
	extra.length > 1 ||
	(extra.length === 1 && !/^--resume=TUIC-\d+$/.test(extra[0]))
) {
	throw Error(
		"Usage: jira-cloud-desktop-e2e.mjs <agent-test-name> <account-id> <project-id> <issue-type-id> <output.json> --write-straus-team-only [--resume=TUIC-N]",
	);
}
const root = resolve(import.meta.dirname, "..");
if (!resolve(output).startsWith(join(homedir(), "Gits/"))) throw Error("Evidence must stay under Gits");
const hash = createHash("sha256").update(instance).digest("hex").slice(0, 16);
const socket = join(homedir(), "Gits/.tuic/tuic-at", hash, `tuic-mcp-${hash}.sock`);
const files = readdirSync(join(root, "dist/assets"));
const asset = (prefix) => {
	const name = files.find((name) => name.startsWith(prefix) && name.endsWith(".js"));
	if (!name) throw Error(`Missing ${prefix} asset`);
	return `/assets/${name}`;
};
const modules = { loader: asset("pluginLoader-"), registry: asset("markdownProviderRegistry-") };
const source = readFileSync(join(root, "dist", modules.loader), "utf8");
const symbol = source.match(/function (\w+)\((\w+)\)\{return \w+\(\{payload:\[\2\]\}\)\}/)?.[1];
const reloadExport = source.match(new RegExp(`(?:\\{|,)${symbol} as (\\w+)`))?.[1];
if (!reloadExport) throw Error("Cannot resolve production reload export");
function request(path, body) {
	return JSON.parse(
		execFileSync(
			"curl",
			[
				"--silent",
				"--show-error",
				"--fail",
				"--max-time",
				"15",
				"--unix-socket",
				socket,
				...(body ? ["-H", "Content-Type: application/json", "--data-binary", JSON.stringify(body)] : []),
				`http://localhost${path}`,
			],
			{ encoding: "utf8", maxBuffer: 2 ** 22 },
		),
	);
}
async function evaluate(fn, input) {
	const marker = randomUUID();
	const result = request("/debug/invoke_js", {
		script: `return(async()=>{try{return{marker:${JSON.stringify(marker)},value:await(${fn.toString()})(${JSON.stringify(input)})}}catch(e){return{marker:${JSON.stringify(marker)},error:String(e)}}})();`,
	});
	if (!result.ok) throw Error(JSON.stringify(result));
	const deadline = Date.now() + 120000;
	while (Date.now() < deadline) {
		for (const entry of request("/logs?source=eval_js&limit=100")) {
			let result;
			try {
				result = JSON.parse(entry.message.split("\n---\n").at(-1));
			} catch {
				continue;
			}
			if (result?.marker !== marker) continue;
			if (result.error) throw Error(result.error);
			return result.value;
		}
		await new Promise((done) => setTimeout(done, 200));
	}
	throw Error("Native WebView result deadline exceeded");
}
const health = request("/health");
if (!health.ok || health.socket_path !== socket) throw Error("Wrong isolated instance");
const result = await evaluate(
	async (input) => {
		const invoke = window.__TAURI__.core.invoke;
		const host = "https://straus-team.atlassian.net";
		const loader = await import(input.modules.loader);
		await invoke("install_plugin_from_folder", { path: `${input.root}/plugins/jira` });
		// Only the selected account is seeded; the default project must come from actual Settings input.
		await invoke("write_plugin_data", {
			pluginId: "jira",
			path: "work-items.json",
			content: JSON.stringify({ accounts: [{ id: input.accountId, host, kind: "basic" }] }),
		});
		await loader[input.reloadExport]("jira");
		const registry = Object.values(await import(input.modules.registry)).find(
			(value) =>
				value?.list && value?.get && value?.register && value.list().some((provider) => provider.providerId === "jira"),
		);
		if (!registry) throw Error("Production Jira provider registry not found");
		const wait = async (fn) => {
			for (let i = 0; i < 300; i++) {
				const value = fn();
				if (value) return value;
				await new Promise((done) => setTimeout(done, 100));
			}
			throw Error("Native settings control deadline exceeded");
		};
		if (![...document.querySelectorAll("button")].some((button) => button.textContent.trim() === "Manage plugins"))
			await window.__TAURI__.event.emit("menu-action", "settings");
		(await wait(() => document.querySelector('button[title="Plugin: jira"]'))).click();
		const field = await wait(() =>
			[...document.querySelectorAll("input")].find((field) =>
				[...document.querySelectorAll("label")].some(
					(label) => label.htmlFor === field.id && label.textContent === "Default project ID",
				),
			),
		);
		field.value = input.projectId;
		field.dispatchEvent(new Event("input", { bubbles: true }));
		field.dispatchEvent(new FocusEvent("blur"));
		let saved;
		for (let i = 0; i < 300; i++) {
			const raw = await invoke("read_plugin_data", { pluginId: "jira", path: "config.json" });
			if (raw && JSON.parse(raw).projectId === input.projectId) {
				saved = JSON.parse(raw);
				break;
			}
			await new Promise((done) => setTimeout(done, 100));
		}
		if (!saved) throw Error("Native settings did not persist project ID");
		await loader[input.reloadExport]("jira");
		const provider = registry.get("jira", input.accountId);
		if (!provider || registry.list("jira").length !== 1) throw Error("Expected exactly one production Jira provider");
		if (!provider.resolveUrl(host + "/browse/TUIC-1")) throw Error("Provider is not bound to authorized site");
		const types = await provider.createFields();
		if (!types[0].options.some((type) => type.id === input.issueType))
			throw Error("Vendor rejected configured project/type");
		const schema = await provider.createFields(input.issueType);
		const body = "Native plugin creation\n\n- First\n- Second\n\n```js\nconst n = 1;\n```\n\nHard  \nbreak";
		const created = input.resume
			? {
					kind: "item",
					value: await provider.detail({ providerId: "jira", accountId: input.accountId, id: input.resume }),
				}
			: await provider.act(null, {
					kind: "create",
					value: {
						title: "Native Jira plugin E2E",
						body,
						itemType: input.issueType,
						fields: { labels: { kind: "labels", value: [{ id: "native-e2e", name: "native-e2e" }] } },
					},
				});
		const ref = created.value.item.reference;
		if (!ref.id.startsWith("TUIC-")) throw Error("Write escaped scratch project");
		if (created.value.body !== body) throw Error("Post-create ADF read-back changed Markdown");
		const comment = await provider.act(ref, { kind: "comment", value: "Native provider comment with hard  \nbreak" });
		if (
			!comment.value.comments.entries.some((entry) => entry.value.body === "Native provider comment with hard  \nbreak")
		)
			throw Error("Post-comment read-back missing");
		const done = comment.value.actions.find((action) => action.kind === "transition" && action.name === "Done");
		if (!done) throw Error("No legal Done transition");
		const moved = await provider.act(ref, { kind: "transition", value: done.id });
		if (moved.value.item.state.name !== "Done") throw Error("Transition did not persist");
		const query = { search: `key = ${ref.id}`, filters: [], savedFilterId: null, sort: null, cursor: null, limit: 25 };
		const page = await provider.list(query);
		if (!page.items.some((item) => item.reference.id === ref.id)) throw Error("Created item not searchable");
		return {
			settings: { source: "actual input and blur", savedProjectId: saved.projectId },
			provider: { providerId: provider.providerId, registrationCount: registry.list("jira").length },
			metadata: {
				issueTypes: types[0].options,
				fields: schema.map((field) => ({ id: field.id, required: field.required })),
			},
			created: {
				resumedAfterCreate: !!input.resume,
				key: created.value.item.key,
				url: created.value.item.url,
				title: created.value.item.title,
				body: created.value.body,
			},
			commentReadBack: true,
			transition: { id: done.id, state: moved.value.item.state.name },
			searchable: true,
			accountBound: true,
			pluginSource: input.root + "/plugins/jira",
		};
	},
	{ root, modules, reloadExport, accountId, projectId, issueType, resume: extra[0]?.slice("--resume=".length) },
);
const report = { date: new Date().toISOString(), instance, socket, native: "aarch64-apple-darwin", result };
writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify(report, null, 2));
