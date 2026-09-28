import { createSignal } from "solid-js";

interface ChatTabs {
	ids: string[];
	active: string;
}

const STORAGE_KEY = "tuic-ai-chat-tabs";
const [byRoot, setByRoot] = createSignal<Record<string, ChatTabs>>({});

function read(): Record<string, ChatTabs> {
	try {
		const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
		if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
		const valid: Record<string, ChatTabs> = {};
		for (const [root, tabs] of Object.entries(raw)) {
			if (!tabs || typeof tabs !== "object") continue;
			const candidate = tabs as Partial<ChatTabs>;
			if (!Array.isArray(candidate.ids) || !candidate.ids.every((id) => typeof id === "string")) continue;
			const ids = [...new Set(candidate.ids)];
			if (ids.length && typeof candidate.active === "string" && ids.includes(candidate.active)) {
				valid[root] = { ids, active: candidate.active };
			}
		}
		return valid;
	} catch {
		return {};
	}
}

function save(root: string, tabs: ChatTabs): void {
	const next = { ...byRoot(), ...read(), [root]: tabs };
	setByRoot(next);
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
	} catch {
		// Keep the open document usable when storage is unavailable.
	}
}

export const aiChatTabs = {
	ids: (root: string) => byRoot()[root]?.ids ?? [],
	active: (root: string) => byRoot()[root]?.active ?? null,
	refresh(): void {
		setByRoot(read());
	},
	ensure(root: string, session: string): void {
		if (byRoot()[root]) return;
		save(root, { ids: [session], active: session });
	},
	add(root: string, session: string): void {
		const ids = byRoot()[root]?.ids ?? [];
		save(root, { ids: ids.includes(session) ? ids : [...ids, session], active: session });
	},
	replace(root: string, session: string): void {
		save(root, { ids: [session], active: session });
	},
	close(root: string, session: string): string | null {
		const current = byRoot()[root];
		if (!current || current.ids.length < 2 || !current.ids.includes(session)) return current?.active ?? null;
		const index = current.ids.indexOf(session);
		const ids = current.ids.filter((id) => id !== session);
		const active = current.active === session ? ids[Math.max(0, index - 1)] : current.active;
		save(root, { ids, active });
		return active;
	},
	/** Recreate a fresh document in tests; the persisted store remains. */
	resetMemory(): void {
		setByRoot({});
	},
};

if (typeof window !== "undefined") {
	aiChatTabs.refresh();
	window.addEventListener("storage", (event) => {
		if (event.key === STORAGE_KEY) aiChatTabs.refresh();
	});
}
