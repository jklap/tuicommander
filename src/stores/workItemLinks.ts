import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import type { WorkItemRef } from "../types/work-items.gen";
import type { WorkItemLink, WorkItemLinks } from "../types/workItemLinks";
import { appLogger } from "./appLogger";

/** Frontend mirror of `workitem_links.json`. The backend file is the source of truth. */
export function createWorkItemLinksStore() {
	const [links, setLinks] = createSignal<WorkItemLinks>({});
	let inflight: Promise<void> | null = null;
	// A local mutation bumps this, so a list response started before it cannot undo it.
	let generation = 0;

	function refresh(): Promise<void> {
		if (inflight) return inflight;
		const started = generation;
		inflight = invoke<WorkItemLinks>("list_workitem_links")
			.then((next) => {
				if (started === generation) setLinks(next ?? {});
			})
			.catch((err) => appLogger.warn("store", "Failed to load work-item links", err))
			.finally(() => {
				inflight = null;
			});
		return inflight;
	}

	// Loaded on first read. Every later change made by this client updates the
	// signal itself, and a removed worktree takes its sidebar row with it.
	let loaded = false;
	function all(): WorkItemLinks {
		if (!loaded) {
			loaded = true;
			void refresh();
		}
		return links();
	}

	function get(repoPath: string, branch: string): WorkItemLink | undefined {
		return all()[repoPath]?.[branch];
	}

	function findByItem(ref: WorkItemRef): { repoPath: string; branch: string; link: WorkItemLink } | undefined {
		for (const [repoPath, branches] of Object.entries(all())) {
			for (const [branch, link] of Object.entries(branches)) {
				const r = link.reference;
				if (r.providerId === ref.providerId && r.accountId === ref.accountId && r.id === ref.id)
					return { repoPath, branch, link };
			}
		}
		return undefined;
	}

	async function link(repoPath: string, branch: string, value: WorkItemLink): Promise<void> {
		await invoke("link_workitem", { repoPath, branch, link: value });
		generation++;
		setLinks((all) => ({ ...all, [repoPath]: { ...all[repoPath], [branch]: value } }));
	}

	async function unlink(repoPath: string, branch: string): Promise<boolean> {
		const removed = await invoke<boolean>("unlink_workitem", { repoPath, branch });
		generation++;
		setLinks((all) => {
			if (!all[repoPath]?.[branch]) return all;
			const { [branch]: _gone, ...rest } = all[repoPath];
			const next = { ...all, [repoPath]: rest };
			if (Object.keys(rest).length === 0) delete next[repoPath];
			return next;
		});
		return removed;
	}

	return { refresh, get, findByItem, link, unlink };
}

export const workItemLinksStore = createWorkItemLinksStore();
