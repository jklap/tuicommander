import { beforeEach, describe, expect, it } from "vitest";
import "../mocks/tauri";
import { commandPaletteStore } from "../../stores/commandPalette";
import type { TerminalMatch } from "../../types";

describe("commandPaletteStore", () => {
	beforeEach(() => {
		commandPaletteStore.close();
		localStorage.clear();
	});

	it("starts closed with empty query", () => {
		expect(commandPaletteStore.state.isOpen).toBe(false);
		expect(commandPaletteStore.state.query).toBe("");
	});

	it("open() sets isOpen to true and clears query", () => {
		commandPaletteStore.setQuery("test");
		commandPaletteStore.open();
		expect(commandPaletteStore.state.isOpen).toBe(true);
		expect(commandPaletteStore.state.query).toBe("");
	});

	it("close() sets isOpen to false and clears query", () => {
		commandPaletteStore.open();
		commandPaletteStore.setQuery("test");
		commandPaletteStore.close();
		expect(commandPaletteStore.state.isOpen).toBe(false);
		expect(commandPaletteStore.state.query).toBe("");
	});

	it("toggle() opens when closed and closes when open", () => {
		commandPaletteStore.toggle();
		expect(commandPaletteStore.state.isOpen).toBe(true);
		commandPaletteStore.toggle();
		expect(commandPaletteStore.state.isOpen).toBe(false);
	});

	it("setQuery() updates the query", () => {
		commandPaletteStore.setQuery("zoom");
		expect(commandPaletteStore.state.query).toBe("zoom");
	});

	it("recordUsage() adds action to recent list", () => {
		commandPaletteStore.recordUsage("zoom-in");
		expect(commandPaletteStore.state.recentActions).toContain("zoom-in");
	});

	it("recordUsage() moves action to front if already present", () => {
		commandPaletteStore.recordUsage("zoom-in");
		commandPaletteStore.recordUsage("zoom-out");
		commandPaletteStore.recordUsage("zoom-in");
		expect(commandPaletteStore.state.recentActions[0]).toBe("zoom-in");
		expect(commandPaletteStore.state.recentActions[1]).toBe("zoom-out");
	});

	it("recordUsage() persists to localStorage", () => {
		commandPaletteStore.recordUsage("zoom-in");
		const stored = JSON.parse(localStorage.getItem("tui-commander-recent-actions") || "[]");
		expect(stored).toContain("zoom-in");
	});

	it("recordUsage() caps at 10 items", () => {
		for (let i = 0; i < 15; i++) {
			commandPaletteStore.recordUsage(`action-${i}`);
		}
		expect(commandPaletteStore.state.recentActions.length).toBeLessThanOrEqual(10);
	});

	describe("content search mode", () => {
		it("mode() returns 'filename' when query starts with !", () => {
			commandPaletteStore.setQuery("!search");
			expect(commandPaletteStore.mode()).toBe("filename");
		});

		it("mode() returns 'content' when query starts with ?", () => {
			commandPaletteStore.setQuery("?search");
			expect(commandPaletteStore.mode()).toBe("content");
		});

		it("mode() returns 'command' for normal queries", () => {
			commandPaletteStore.setQuery("zoom");
			expect(commandPaletteStore.mode()).toBe("command");
		});

		it("mode() returns 'command' for empty query", () => {
			commandPaletteStore.setQuery("");
			expect(commandPaletteStore.mode()).toBe("command");
		});

		it("searchQuery() strips ! prefix", () => {
			commandPaletteStore.setQuery("!findme");
			expect(commandPaletteStore.searchQuery()).toBe("findme");
		});

		it("searchQuery() strips ? prefix", () => {
			commandPaletteStore.setQuery("?findme");
			expect(commandPaletteStore.searchQuery()).toBe("findme");
		});

		it("searchQuery() trims leading space after prefix", () => {
			commandPaletteStore.setQuery("! readme");
			expect(commandPaletteStore.searchQuery()).toBe("readme");
			commandPaletteStore.setQuery("?  foo");
			expect(commandPaletteStore.searchQuery()).toBe("foo");
		});

		it("searchQuery() returns empty for command queries", () => {
			commandPaletteStore.setQuery("zoom");
			expect(commandPaletteStore.searchQuery()).toBe("");
		});

		it("close() resets search state", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("?test");
			commandPaletteStore.close();
			expect(commandPaletteStore.state.contentResults).toEqual([]);
			expect(commandPaletteStore.state.contentSearching).toBe(false);
			expect(commandPaletteStore.state.contentError).toBeNull();
			expect(commandPaletteStore.state.filenameResults).toEqual([]);
			expect(commandPaletteStore.state.filenameSearching).toBe(false);
		});

		it("switching from ? to command clears content state", () => {
			commandPaletteStore.setQuery("?test");
			commandPaletteStore.setQuery("test");
			expect(commandPaletteStore.state.contentResults).toEqual([]);
			expect(commandPaletteStore.state.contentSearching).toBe(false);
		});

		it("switching from ! to command clears filename state", () => {
			commandPaletteStore.setQuery("!test");
			commandPaletteStore.setQuery("test");
			expect(commandPaletteStore.state.filenameResults).toEqual([]);
			expect(commandPaletteStore.state.filenameSearching).toBe(false);
		});
	});

	describe("terminal search mode (~)", () => {
		it("mode() returns 'terminal' when query starts with ~", () => {
			commandPaletteStore.setQuery("~error");
			expect(commandPaletteStore.mode()).toBe("terminal");
		});

		it("searchQuery() strips ~ prefix and trims", () => {
			commandPaletteStore.setQuery("~error");
			expect(commandPaletteStore.searchQuery()).toBe("error");
			commandPaletteStore.setQuery("~  spaced");
			expect(commandPaletteStore.searchQuery()).toBe("spaced");
		});

		it("close() resets terminal search state", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("~test");
			commandPaletteStore.close();
			expect(commandPaletteStore.state.terminalResults).toEqual([]);
			expect(commandPaletteStore.state.terminalSearching).toBe(false);
		});

		it("switching from ~ to command clears terminal state", () => {
			commandPaletteStore.setQuery("~test");
			commandPaletteStore.setQuery("test");
			expect(commandPaletteStore.state.terminalResults).toEqual([]);
			expect(commandPaletteStore.state.terminalSearching).toBe(false);
		});

		it("TerminalMatch type is importable and well-shaped", () => {
			const match: TerminalMatch = {
				terminalId: "term-1",
				terminalName: "Shell",
				lineIndex: 42,
				lineText: "error: something failed",
				matchStart: 0,
				matchEnd: 5,
			};
			expect(match.terminalId).toBe("term-1");
			expect(match.matchEnd).toBe(5);
		});
	});

	describe("openWithQuery()", () => {
		it("opens palette with pre-filled query", () => {
			commandPaletteStore.openWithQuery("~ ");
			expect(commandPaletteStore.state.isOpen).toBe(true);
			expect(commandPaletteStore.state.query).toBe("~ ");
		});

		it("sets terminal mode when opened with ~", () => {
			commandPaletteStore.openWithQuery("~ ");
			expect(commandPaletteStore.mode()).toBe("terminal");
		});

		it("sets filename mode when opened with !", () => {
			commandPaletteStore.openWithQuery("! ");
			expect(commandPaletteStore.mode()).toBe("filename");
		});

		it("sets content mode when opened with ?", () => {
			commandPaletteStore.openWithQuery("? ");
			expect(commandPaletteStore.mode()).toBe("content");
		});
	});

	describe("scope()", () => {
		it("defaults to 'all' when opened", () => {
			commandPaletteStore.open();
			expect(commandPaletteStore.scope()).toBe("all");
		});

		it("reports the search scope derived from a typed prefix, without touching a chip", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("! foo");
			expect(commandPaletteStore.scope()).toBe("files");
			commandPaletteStore.setQuery("? foo");
			expect(commandPaletteStore.scope()).toBe("content");
			commandPaletteStore.setQuery("~ foo");
			expect(commandPaletteStore.scope()).toBe("terminals");
		});

		it("close() resets the action filter back to 'all'", () => {
			commandPaletteStore.open();
			commandPaletteStore.setScope("prompts");
			commandPaletteStore.close();
			commandPaletteStore.open();
			expect(commandPaletteStore.scope()).toBe("all");
		});
	});

	describe("setScope()", () => {
		it("switches between action-category scopes without touching the query prefix", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("zoom");
			commandPaletteStore.setScope("actions");
			expect(commandPaletteStore.scope()).toBe("actions");
			expect(commandPaletteStore.state.query).toBe("zoom");
			expect(commandPaletteStore.mode()).toBe("command");

			commandPaletteStore.setScope("prompts");
			expect(commandPaletteStore.scope()).toBe("prompts");
			expect(commandPaletteStore.state.query).toBe("zoom");
		});

		it("rewrites the query prefix when switching to a search scope, carrying the typed text over", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("readme");
			commandPaletteStore.setScope("files");
			expect(commandPaletteStore.mode()).toBe("filename");
			expect(commandPaletteStore.state.query).toBe("! readme");
			expect(commandPaletteStore.scope()).toBe("files");
		});

		it("carries typed text across a switch between two search scopes", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("! readme");
			commandPaletteStore.setScope("content");
			expect(commandPaletteStore.mode()).toBe("content");
			expect(commandPaletteStore.state.query).toBe("? readme");
		});

		it("switching from a search scope back to an action scope strips the prefix and carries the text", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("~ output");
			commandPaletteStore.setScope("all");
			expect(commandPaletteStore.mode()).toBe("command");
			expect(commandPaletteStore.state.query).toBe("output");
			expect(commandPaletteStore.scope()).toBe("all");
		});

		it("switching to an empty search scope still primes the prefix for typing", () => {
			commandPaletteStore.open();
			commandPaletteStore.setScope("terminals");
			expect(commandPaletteStore.state.query).toBe("~ ");
			expect(commandPaletteStore.mode()).toBe("terminal");
		});
	});

	describe("cycleScope()", () => {
		it("cycles forward through all six scopes in order and wraps", () => {
			commandPaletteStore.open();
			const order: ReturnType<typeof commandPaletteStore.scope>[] = [
				"all",
				"actions",
				"prompts",
				"files",
				"content",
				"terminals",
			];
			for (let i = 0; i < order.length; i++) {
				expect(commandPaletteStore.scope()).toBe(order[i]);
				commandPaletteStore.cycleScope(1);
			}
			// Wrapped back to the start.
			expect(commandPaletteStore.scope()).toBe("all");
		});

		it("cycles backward and wraps at the start", () => {
			commandPaletteStore.open();
			expect(commandPaletteStore.scope()).toBe("all");
			commandPaletteStore.cycleScope(-1);
			expect(commandPaletteStore.scope()).toBe("terminals");
			commandPaletteStore.cycleScope(-1);
			expect(commandPaletteStore.scope()).toBe("content");
		});

		it("preserves typed text while cycling through several scopes", () => {
			commandPaletteStore.open();
			commandPaletteStore.setQuery("draft");
			commandPaletteStore.cycleScope(1); // all -> actions
			expect(commandPaletteStore.state.query).toBe("draft");
			commandPaletteStore.cycleScope(1); // actions -> prompts
			expect(commandPaletteStore.state.query).toBe("draft");
			commandPaletteStore.cycleScope(1); // prompts -> files
			expect(commandPaletteStore.state.query).toBe("! draft");

			// Landing on "files" with real text schedules a debounced filename
			// search (SEARCH_DEBOUNCE_MS); close() clears it via cleanupSearch()
			// so it doesn't outlive this test as an async-leak false positive —
			// the same cleanup every other test in this file relies on its own
			// `beforeEach`'s close() call for, but this is the last test in the
			// file, so nothing downstream would do it.
			commandPaletteStore.close();
		});
	});
});
