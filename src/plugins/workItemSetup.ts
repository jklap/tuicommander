/** Setup descriptors for the work-item plugins, so core can edit their data/work-items.json without the user writing JSON. */
import { invoke } from "../invoke";
import { reloadPlugin } from "./pluginLoader";

export interface SetupField {
	key: string;
	label: string;
	placeholder: string;
	required: boolean;
	/** One `id name` entry per line, stored as `[{id, name}]`. */
	boards?: boolean;
}

export interface WorkItemPluginSetup {
	pluginId: string;
	/** Provider id the plugin registers, used to find the plugin behind a registered provider. */
	providerId: string;
	name: string;
	/** What the Tasks tab says is missing while the plugin has no account. */
	missing: string;
	/** Why this plugin matches the user, shown beside its install button. */
	purpose: string;
	/** Written into every saved account, not shown in the form. */
	constants: Record<string, string>;
	fields: readonly SetupField[];
}

const accountId = (placeholder: string): SetupField => ({ key: "id", label: "Account", placeholder, required: true });

export const WORK_ITEM_PLUGINS: readonly WorkItemPluginSetup[] = [
	{
		pluginId: "github-work",
		providerId: "github",
		name: "GitHub Work",
		missing: "a repository",
		purpose: "GitHub issues and pull requests",
		constants: {},
		fields: [
			accountId("github.com"),
			{ key: "repository", label: "Repository", placeholder: "owner/repo", required: true },
		],
	},
	{
		pluginId: "gitlab",
		providerId: "gitlab",
		name: "GitLab",
		missing: "a host and a project",
		purpose: "GitLab issues",
		constants: {},
		fields: [
			accountId("gitlab.com"),
			{ key: "host", label: "Host", placeholder: "https://gitlab.com", required: true },
			{ key: "project", label: "Project", placeholder: "group/project", required: false },
			{ key: "boards", label: "Boards", placeholder: "12 Sprint board", required: false, boards: true },
		],
	},
	{
		pluginId: "jira",
		providerId: "jira",
		name: "Jira",
		missing: "a site and a project",
		purpose: "Jira issues",
		constants: { kind: "basic" },
		fields: [
			accountId("my-site"),
			{ key: "host", label: "Host", placeholder: "https://site.atlassian.net", required: true },
			{ key: "projectId", label: "Project", placeholder: "10000", required: false },
			{ key: "boards", label: "Boards", placeholder: "1 Sprint board", required: false, boards: true },
		],
	},
];

type Values = Record<string, string>;
type Account = Record<string, unknown>;
const CONFIG = "work-items.json";

export const setupFor = (pluginId: string) => WORK_ITEM_PLUGINS.find((setup) => setup.pluginId === pluginId);
export const setupForProvider = (providerId: string) =>
	WORK_ITEM_PLUGINS.find((setup) => setup.providerId === providerId);

const parseBoards = (text: string) =>
	text
		.split("\n")
		.map((line) => line.trim())
		.filter(Boolean)
		.map((line) => {
			const [id = "", ...name] = line.split(/\s+/);
			return { id, name: name.join(" ") || id };
		});

const formatBoards = (boards: unknown) =>
	Array.isArray(boards)
		? boards
				.map((board: { id: string; name: string }) =>
					board.name === board.id ? board.id : `${board.id} ${board.name}`,
				)
				.join("\n")
		: "";

export function missingFields(setup: WorkItemPluginSetup, values: Values): string[] {
	return setup.fields.filter((field) => field.required && !values[field.key]?.trim()).map((field) => field.label);
}

export function accountFromValues(setup: WorkItemPluginSetup, values: Values): Account {
	const account: Account = { ...setup.constants };
	for (const field of setup.fields) {
		const text = (values[field.key] ?? "").trim();
		if (!text) continue;
		account[field.key] = field.boards ? parseBoards(text) : text;
	}
	return account;
}

export function valuesFromAccount(setup: WorkItemPluginSetup, account: Account | undefined): Values {
	return Object.fromEntries(
		setup.fields.map((field) => {
			const stored = account?.[field.key];
			if (field.boards) return [field.key, formatBoards(stored)];
			return [field.key, typeof stored === "string" ? stored : field.key === "id" ? field.placeholder : ""];
		}),
	);
}

function parseConfig(raw: string | null): { accounts?: Account[] } & Record<string, unknown> {
	if (!raw) return {};
	try {
		const parsed: unknown = JSON.parse(raw);
		if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) return parsed as Record<string, unknown>;
	} catch {
		// fall through to the shared error
	}
	throw new Error(`${CONFIG} is not valid JSON; fix or delete it before using the form`);
}

/** Updates form-owned keys on the matching account, preserving the rest of the config. */
export function upsertAccount(raw: string | null, account: Account, editedKeys = Object.keys(account)): string {
	const config = parseConfig(raw);
	const accounts = Array.isArray(config.accounts) ? config.accounts : [];
	const index = accounts.findIndex((existing) => existing.id === account.id);
	const next =
		index < 0
			? [...accounts, account]
			: accounts.map((existing, i) => {
					if (i !== index) return existing;
					const merged = { ...account, ...existing };
					for (const key of editedKeys) {
						if (key in account) merged[key] = account[key];
						else delete merged[key];
					}
					return merged;
				});
	return JSON.stringify({ ...config, accounts: next }, null, 2);
}

const readRaw = (pluginId: string) => invoke<string | null>("read_plugin_data", { pluginId, path: CONFIG });

/** Form values for the first saved account, or the defaults when the plugin has none. */
export async function loadWorkItemSetup(setup: WorkItemPluginSetup): Promise<Values> {
	const config = parseConfig(await readRaw(setup.pluginId));
	return valuesFromAccount(setup, Array.isArray(config.accounts) ? config.accounts[0] : undefined);
}

export async function saveWorkItemSetup(setup: WorkItemPluginSetup, values: Values): Promise<void> {
	const content = upsertAccount(
		await readRaw(setup.pluginId),
		accountFromValues(setup, values),
		setup.fields.map((field) => field.key),
	);
	await invoke("write_plugin_data", { pluginId: setup.pluginId, path: CONFIG, content });
	// Writes under data/ are not watched, so the plugin only sees them after an explicit reload.
	await reloadPlugin(setup.pluginId);
}
