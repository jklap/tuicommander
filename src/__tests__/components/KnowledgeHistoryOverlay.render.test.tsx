import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";

// The overlay mounts once at app startup (App.tsx) and stays mounted for the
// app's lifetime — only its content is gated by `visible()`. That means
// `autofocus` on its search input never applies (the node is never freshly
// inserted), which existing coverage (KnowledgeHistoryOverlay.test.ts) never
// caught since it only exercises the pure `copyToClipboard` helper.

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue([]) }));

import { KnowledgeHistoryOverlay } from "../../components/KnowledgeHistory/KnowledgeHistoryOverlay";
import { uiStore } from "../../stores/ui";

describe("KnowledgeHistoryOverlay focus-on-open", () => {
	afterEach(() => {
		uiStore.setKnowledgeHistoryOverlayVisible(false);
	});

	it("focuses the search input when opened (autofocus is inert on this always-mounted overlay)", async () => {
		const { container } = render(() => <KnowledgeHistoryOverlay />);
		expect(container.querySelector('[placeholder="Search commands, output, errors…"]')).toBeNull();

		uiStore.setKnowledgeHistoryOverlayVisible(true);

		const searchInput = await waitFor(() => {
			const el = container.querySelector('[placeholder="Search commands, output, errors…"]') as HTMLInputElement | null;
			expect(el).not.toBeNull();
			return el!;
		});
		await waitFor(() => expect(document.activeElement).toBe(searchInput));
	});
});
