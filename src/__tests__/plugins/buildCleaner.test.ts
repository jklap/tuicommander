/**
 * Tests for the build-cleaner plugin's pure logic.
 *
 * Unlike claude-wakeup (whose logic is duplicated here because it had no
 * exports), build-cleaner exports its pure helpers as named exports alongside
 * the default plugin object, so we import and test the REAL implementation.
 * The plugin loader only reads `.default`, so these named exports are runtime-inert.
 */
import { describe, expect, it, vi } from "vitest";
// @ts-expect-error — untyped plugin JS module (submodule), imported for its pure exports.
import * as plugin from "../../../plugins/build-cleaner/main.js";

const {
	fmtBytes,
	fmtAge,
	basename,
	relPath,
	evaluateThresholds,
	applyRemoval,
	tickerPriority,
	configToForm,
	formToConfig,
	buildPanelHtml,
} = plugin as {
	fmtBytes: (n: number) => string;
	fmtAge: (mtime: number, now: number) => string;
	basename: (p: string) => string;
	relPath: (child: string, repo: string) => string;
	evaluateThresholds: (
		entries: Array<{
			kind: string;
			size_bytes: number;
			trimmable_bytes?: number;
			last_modified_secs: number;
			repo: string;
			path: string;
		}>,
		cfg: Record<string, unknown>,
		now: number,
	) => { severity: string; totalBytes: number; trimmableBytes: number; staleCount: number; largest: unknown };
	applyRemoval: <T>(entries: T[], path: string, trim: boolean, reclaimedBytes?: number) => T[];
	tickerPriority: (sev: string) => number;
	configToForm: (cfg: Record<string, unknown>) => Record<string, unknown>;
	formToConfig: (form: Record<string, unknown>, base: Record<string, unknown>) => Record<string, unknown>;
	buildPanelHtml: (
		entries: unknown[] | null,
		cfg: Record<string, unknown>,
		now: number,
		opts?: { scanning?: boolean },
	) => string;
};

const GIB = 1024 * 1024 * 1024;
const HOUR = 3600;
const NOW = 1_000_000_000; // fixed clock

const buildCleanerPlugin = plugin.default as {
	onload: (host: Record<string, unknown>) => Promise<void>;
	onunload: () => void;
};

/** Every kind the scanner can emit — mirrors ALL_KINDS in main.js. */
const KINDS = [
	"rust",
	"maven",
	"node",
	"jscache",
	"python",
	"dotnet",
	"gradle",
	"cmake",
	"swift",
	"flutter",
	"terraform",
	"elixir",
	"zig",
	"haskell",
	"php",
];

const CFG = {
	perArtifactWarnBytes: 5 * GIB,
	totalWarnBytes: 50 * GIB,
	totalCriticalBytes: 150 * GIB,
	hotWindowSecs: 24 * HOUR,
	pollIntervalMs: 3_600_000,
	enabledKinds: [...KINDS],
};

/** An artifact built `ageHours` ago. `trimmableGib` defaults to 0 — the value
 *  the backend reports for kinds with no separable intermediates. */
function art(kind: string, gib: number, ageHours: number, repo = "/home/u/repoA", name = kind, trimmableGib = 0) {
	return {
		kind,
		size_bytes: gib * GIB,
		trimmable_bytes: trimmableGib * GIB,
		last_modified_secs: NOW - ageHours * HOUR,
		repo,
		path: `${repo}/${name}`,
	};
}

describe("build-cleaner pure helpers", () => {
	describe("fmtBytes", () => {
		it("formats binary units", () => {
			expect(fmtBytes(0)).toBe("0 B");
			expect(fmtBytes(512)).toBe("512 B");
			expect(fmtBytes(1024)).toBe("1 KiB");
			expect(fmtBytes(1536)).toBe("1.5 KiB");
			expect(fmtBytes(5 * GIB)).toBe("5 GiB");
			expect(fmtBytes(1.5 * 1024 * GIB)).toBe("1.5 TiB");
		});
		it("guards against negative/NaN", () => {
			expect(fmtBytes(-1)).toBe("0 B");
			expect(fmtBytes(Number.NaN)).toBe("0 B");
		});
	});

	describe("fmtAge", () => {
		it("returns em-dash for unknown mtime", () => {
			expect(fmtAge(0, NOW)).toBe("—");
		});
		it("formats minutes/hours/days/months", () => {
			expect(fmtAge(NOW - 120, NOW)).toBe("2m");
			expect(fmtAge(NOW - 3 * HOUR, NOW)).toBe("3h");
			expect(fmtAge(NOW - 5 * 24 * HOUR, NOW)).toBe("5d");
			expect(fmtAge(NOW - 90 * 24 * HOUR, NOW)).toBe("3mo");
		});
	});

	describe("basename / relPath", () => {
		it("handles posix and windows separators", () => {
			expect(basename("/home/u/repoA/target")).toBe("target");
			expect(basename("C:\\code\\repoA\\target")).toBe("target");
		});
		it("relativizes child against repo root", () => {
			expect(relPath("/home/u/repoA/sub/node_modules", "/home/u/repoA")).toBe("sub/node_modules");
			expect(relPath("/home/u/repoA/target", "/home/u/repoA")).toBe("target");
			expect(relPath("/elsewhere/target", "/home/u/repoA")).toBe("/elsewhere/target");
		});
	});

	describe("evaluateThresholds", () => {
		it("returns none below all thresholds", () => {
			const res = evaluateThresholds([art("rust", 2, 48)], CFG, NOW);
			expect(res.severity).toBe("none");
			expect(res.totalBytes).toBe(2 * GIB);
			expect(res.staleCount).toBe(1);
		});

		it("excludes artifacts newer than the hot-window from the total", () => {
			// 100 GiB but built 1h ago → excluded → none
			const res = evaluateThresholds([art("rust", 100, 1)], CFG, NOW);
			expect(res.severity).toBe("none");
			expect(res.totalBytes).toBe(0);
			expect(res.staleCount).toBe(0);
		});

		it("warns when total crosses totalWarnBytes (stale only)", () => {
			const res = evaluateThresholds([art("rust", 30, 48, "/r/a"), art("node", 25, 48, "/r/b")], CFG, NOW);
			expect(res.severity).toBe("warn");
			expect(res.totalBytes).toBe(55 * GIB);
		});

		it("warns on a single oversized artifact even under the total warn", () => {
			const res = evaluateThresholds([art("rust", 6, 48)], CFG, NOW);
			expect(res.severity).toBe("warn"); // 6 GiB ≥ perArtifactWarnBytes (5), total 6 < 50
		});

		it("escalates to critical past totalCriticalBytes", () => {
			const res = evaluateThresholds([art("rust", 120, 48, "/r/a"), art("node", 40, 48, "/r/b")], CFG, NOW);
			expect(res.severity).toBe("critical");
			expect(res.totalBytes).toBe(160 * GIB);
		});

		it("ignores disabled kinds", () => {
			const cfg = { ...CFG, enabledKinds: ["node"] };
			const res = evaluateThresholds([art("rust", 100, 48)], cfg, NOW);
			expect(res.severity).toBe("none");
			expect(res.totalBytes).toBe(0);
		});

		it("reports the largest stale artifact", () => {
			const big = art("rust", 30, 48, "/r/a", "target");
			const res = evaluateThresholds([art("node", 10, 48), big], CFG, NOW);
			expect((res.largest as { size_bytes: number }).size_bytes).toBe(30 * GIB);
		});
	});

	describe("tickerPriority", () => {
		it("escalates with severity", () => {
			expect(tickerPriority("none")).toBe(10);
			expect(tickerPriority("warn")).toBe(50);
			expect(tickerPriority("critical")).toBe(90);
		});
	});

	describe("config round-trip", () => {
		it("configToForm converts bytes/secs/ms to GiB/hours/minutes", () => {
			const form = configToForm(CFG);
			expect(form.perArtifactWarnGiB).toBe(5);
			expect(form.totalWarnGiB).toBe(50);
			expect(form.hotWindowHours).toBe(24);
			expect(form.pollIntervalMinutes).toBe(60);
		});

		it("formToConfig is the inverse and clamps invalid values", () => {
			const form = configToForm(CFG);
			const back = formToConfig(form, CFG);
			expect(back.perArtifactWarnBytes).toBe(CFG.perArtifactWarnBytes);
			expect(back.totalWarnBytes).toBe(CFG.totalWarnBytes);
			expect(back.hotWindowSecs).toBe(CFG.hotWindowSecs);
			expect(back.pollIntervalMs).toBe(CFG.pollIntervalMs);
		});

		it("formToConfig rejects non-positive numbers, falling back to base", () => {
			const back = formToConfig({ perArtifactWarnGiB: -3, totalWarnGiB: 0, pollIntervalMinutes: -1 }, CFG);
			expect(back.perArtifactWarnBytes).toBe(CFG.perArtifactWarnBytes);
			expect(back.totalWarnBytes).toBe(CFG.totalWarnBytes);
			expect(back.pollIntervalMs).toBe(CFG.pollIntervalMs);
		});

		it("formToConfig floors the poll interval at 5 minutes", () => {
			const back = formToConfig({ pollIntervalMinutes: 1 }, CFG);
			expect(back.pollIntervalMs).toBe(5 * 60000);
		});

		it("formToConfig drops unknown kinds and falls back if empty", () => {
			expect(formToConfig({ enabledKinds: ["rust", "bogus"] }, CFG).enabledKinds).toEqual(["rust"]);
			expect(formToConfig({ enabledKinds: ["bogus"] }, CFG).enabledKinds).toEqual(KINDS);
		});
	});

	describe("buildPanelHtml", () => {
		it("renders empty-state when no visible artifacts", () => {
			const html = buildPanelHtml([], CFG, NOW);
			expect(html).toContain("empty-state");
			expect(html).toContain("No build artifacts");
		});

		it("groups by repo, shows totals and a Clean button per artifact", () => {
			const entries = [
				art("rust", 30, 48, "/home/u/repoA", "target"),
				art("node", 10, 48, "/home/u/repoB", "node_modules"),
			];
			const html = buildPanelHtml(entries, CFG, NOW);
			expect(html).toContain("repoA");
			expect(html).toContain("repoB");
			expect(html).toContain("dash-stat");
			expect(html).toContain('class="num"');
			expect(html).toContain('data-path="/home/u/repoA/target"');
			expect(html).toContain(">Clean<");
			// largest repo (repoA, 30 GiB) rendered before repoB (10 GiB)
			expect(html.indexOf("repoA")).toBeLessThan(html.indexOf("repoB"));
		});

		it("marks hot (recently built) artifacts with a badge", () => {
			const html = buildPanelHtml([art("rust", 30, 1)], CFG, NOW); // 1h old < 24h window
			expect(html).toContain("badge-hot");
			expect(html).toContain(">recent<");
		});

		it("renders a scanning placeholder for null entries (no scan yet)", () => {
			const html = buildPanelHtml(null, CFG, NOW);
			expect(html).toContain("Scanning repositories");
			expect(html).toContain("Scanning…");
			expect(html).toContain("disabled");
			expect(html).not.toContain("dash-stat-grid");
		});

		it("disables the Rescan button while a scan is in flight over cached entries", () => {
			const html = buildPanelHtml([art("rust", 30, 48)], CFG, NOW, { scanning: true });
			expect(html).toContain("Scanning…");
			expect(html).toContain("disabled");
			expect(html).toContain("dash-stat-grid"); // cached data still rendered
		});

		it("aligns columns across per-repo tables via a shared colgroup", () => {
			const html = buildPanelHtml([art("rust", 30, 48)], CFG, NOW);
			expect(html).toContain("<colgroup>");
			expect(html).toContain("table-layout: fixed");
		});

		it("does not rely on window.confirm (blocked by iframe sandbox)", () => {
			const html = buildPanelHtml([art("rust", 30, 48)], CFG, NOW);
			expect(html).not.toContain("window.confirm");
			expect(html).toContain("armed"); // two-step arm/confirm flow
		});

		it("escapes paths to prevent HTML injection", () => {
			const evil = art("rust", 1, 48, "/home/u/repoA", '"><script>alert(1)</script>');
			const html = buildPanelHtml([evil], CFG, NOW);
			expect(html).not.toContain("<script>alert(1)</script>");
			expect(html).toContain("&lt;script&gt;");
		});
	});

	describe("trim", () => {
		it("totals the trimmable share of stale artifacts only", () => {
			const res = evaluateThresholds(
				[
					art("rust", 30, 48, "/home/u/repoA", "target", 29.5),
					// Built 1h ago — inside the hot window, so excluded from both totals.
					art("rust", 10, 1, "/home/u/repoB", "target", 9.8),
					// No separable intermediates: counts to the total, not to trimmable.
					art("node", 2, 48, "/home/u/repoA", "node_modules"),
				],
				CFG,
				NOW,
			);
			expect(res.totalBytes).toBe(32 * GIB);
			expect(res.trimmableBytes).toBe(29.5 * GIB);
		});

		it("offers Trim only where the backend found intermediates", () => {
			const html = buildPanelHtml(
				[art("rust", 30, 48, "/home/u/repoA", "target", 29.5), art("node", 2, 48, "/home/u/repoA", "node_modules")],
				CFG,
				NOW,
			);
			expect(html).toContain('data-action="trim" data-path="/home/u/repoA/target"');
			expect(html).not.toContain('data-action="trim" data-path="/home/u/repoA/node_modules"');
			// Clean stays available for both — trim is an addition, not a replacement.
			expect(html).toContain('data-action="delete" data-path="/home/u/repoA/target"');
			expect(html).toContain('data-action="delete" data-path="/home/u/repoA/node_modules"');
			expect(html).toContain("Safe to trim");
		});

		it("keeps a trimmed entry listed and drops a cleaned one", () => {
			const entries = [
				art("rust", 30, 48, "/home/u/repoA", "target", 29.5),
				art("node", 2, 48, "/home/u/repoA", "node_modules"),
			];

			// A trim leaves the executables on disk, so the entry must survive with
			// only the reclaimed bytes subtracted — dropping it would hide them.
			// The number comes from the backend's measurement during the delete,
			// not from the scan's estimate: a build in between moves that estimate,
			// and subtracting it would publish a total nothing measured.
			const trimmed = applyRemoval(entries, "/home/u/repoA/target", true, 29.5 * GIB);
			expect(trimmed).toHaveLength(2);
			expect(trimmed[0].size_bytes).toBe(0.5 * GIB);
			expect(trimmed[0].trimmable_bytes).toBe(0);
			expect(entries[0].size_bytes).toBe(30 * GIB); // input untouched

			// The stale estimate said 29.5 GiB; the trim actually reclaimed 20.
			const partial = applyRemoval(entries, "/home/u/repoA/target", true, 20 * GIB);
			expect(partial[0].size_bytes).toBe(10 * GIB);

			const cleaned = applyRemoval(entries, "/home/u/repoA/target", false);
			expect(cleaned).toHaveLength(1);
			expect(cleaned[0].path).toBe("/home/u/repoA/node_modules");
		});

		it("routes a trim message to trimBuildArtifact, not delete", async () => {
			vi.useFakeTimers();
			let handlePanelMessage: ((message: unknown) => Promise<void>) | undefined;
			const entry = art("rust", 30, 48, "/home/u/repoA", "target", 29.5);
			const host = {
				getRepos: vi.fn(() => [{ path: "/home/u/repoA" }]),
				scanBuildArtifacts: vi.fn(async () => [entry]),
				trimBuildArtifact: vi.fn(async () => 29.5 * GIB),
				deleteBuildArtifact: vi.fn(async () => undefined),
				invoke: vi.fn(async (_command: string) => null),
				registerSection: vi.fn(),
				registerDashboard: vi.fn(),
				openPanel: vi.fn((options: { onMessage: (message: unknown) => Promise<void> }) => {
					handlePanelMessage = options.onMessage;
					return { update: vi.fn() };
				}),
				clearTicker: vi.fn(),
				removeItem: vi.fn(),
				addItem: vi.fn(),
				setTicker: vi.fn(),
				log: vi.fn(),
			};

			try {
				await buildCleanerPlugin.onload(host);
				await Promise.resolve();
				// registerDashboard is a no-op mock here; drive the panel directly.
				const dashboard = host.registerDashboard.mock.calls[0][0] as { open: () => Promise<void> };
				await dashboard.open();

				await handlePanelMessage?.({ action: "trim", path: "/home/u/repoA/target" });

				expect(host.trimBuildArtifact).toHaveBeenCalledWith("/home/u/repoA/target", ["/home/u/repoA"]);
				expect(host.deleteBuildArtifact).not.toHaveBeenCalled();
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});
	});

	describe("lifecycle", () => {
		it("forces only an explicit dashboard refresh", async () => {
			vi.useFakeTimers();
			let openDashboard: (() => Promise<void>) | undefined;
			let handlePanelMessage: ((message: unknown) => Promise<void>) | undefined;
			const panel = { update: vi.fn() };
			const host = {
				getRepos: vi.fn(() => [{ path: "/repo" }]),
				scanBuildArtifacts: vi.fn(async () => []),
				invoke: vi.fn(async (_command: string) => null),
				registerSection: vi.fn(),
				registerDashboard: vi.fn((dashboard: { open: () => Promise<void> }) => {
					openDashboard = dashboard.open;
				}),
				openPanel: vi.fn((options: { onMessage: (message: unknown) => Promise<void> }) => {
					handlePanelMessage = options.onMessage;
					return panel;
				}),
				clearTicker: vi.fn(),
				removeItem: vi.fn(),
				addItem: vi.fn(),
				setTicker: vi.fn(),
				log: vi.fn(),
			};

			try {
				await buildCleanerPlugin.onload(host);
				await Promise.resolve();
				expect(host.scanBuildArtifacts).toHaveBeenLastCalledWith(["/repo"], {
					forceRefresh: false,
				});

				host.scanBuildArtifacts.mockClear();
				await openDashboard?.();
				expect(host.scanBuildArtifacts).toHaveBeenLastCalledWith(["/repo"], {
					forceRefresh: false,
				});

				host.scanBuildArtifacts.mockClear();
				await handlePanelMessage?.({ action: "refresh" });
				expect(host.scanBuildArtifacts).toHaveBeenCalledOnce();
				expect(host.scanBuildArtifacts).toHaveBeenLastCalledWith(["/repo"], {
					forceRefresh: true,
				});
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});

		it("does not update the unloaded host after an in-flight poll completes", async () => {
			vi.useFakeTimers();
			let resolveScan: ((entries: unknown[]) => void) | undefined;
			const scanResult = new Promise<unknown[]>((resolve) => {
				resolveScan = resolve;
			});
			const host = {
				getRepos: vi.fn(() => [{ path: "/repo" }]),
				scanBuildArtifacts: vi.fn(() => scanResult),
				invoke: vi.fn(async () => null),
				registerSection: vi.fn(),
				registerDashboard: vi.fn(),
				clearTicker: vi.fn(),
				removeItem: vi.fn(),
				addItem: vi.fn(),
				setTicker: vi.fn(),
				log: vi.fn(),
			};

			try {
				await buildCleanerPlugin.onload(host);
				expect(host.scanBuildArtifacts).toHaveBeenCalledOnce();

				buildCleanerPlugin.onunload();
				const clearCallsAfterUnload = host.clearTicker.mock.calls.length;
				const removeCallsAfterUnload = host.removeItem.mock.calls.length;

				resolveScan?.([]);
				await Promise.resolve();
				await Promise.resolve();

				expect(host.clearTicker).toHaveBeenCalledTimes(clearCallsAfterUnload);
				expect(host.removeItem).toHaveBeenCalledTimes(removeCallsAfterUnload);
				expect(host.addItem).not.toHaveBeenCalled();
				expect(host.setTicker).not.toHaveBeenCalled();
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});
	});

	describe("demand-gated poll", () => {
		/** Mock host that records the panel callbacks the plugin registers. */
		function makeHost() {
			const panel = { update: vi.fn() };
			const captured: {
				openDashboard?: () => Promise<void>;
				onVisibilityChange?: (visible: boolean) => void;
				onClose?: () => void;
				onMessage?: (message: unknown) => Promise<void>;
			} = {};
			const host = {
				getRepos: vi.fn(() => [{ path: "/repo" }]),
				scanBuildArtifacts: vi.fn(async () => []),
				invoke: vi.fn(async (_command: string) => null),
				registerSection: vi.fn(),
				registerDashboard: vi.fn((dashboard: { open: () => Promise<void> }) => {
					captured.openDashboard = dashboard.open;
				}),
				openPanel: vi.fn(
					(options: {
						onMessage?: (message: unknown) => Promise<void>;
						onVisibilityChange?: (visible: boolean) => void;
						onClose?: () => void;
					}) => {
						captured.onMessage = options.onMessage;
						captured.onVisibilityChange = options.onVisibilityChange;
						captured.onClose = options.onClose;
						return panel;
					},
				),
				clearTicker: vi.fn(),
				removeItem: vi.fn(),
				addItem: vi.fn(),
				setTicker: vi.fn(),
				log: vi.fn(),
			};
			return { host, panel, captured };
		}

		const CADENCE = CFG.pollIntervalMs;

		it("never walks the filesystem again when nobody opens the panel", async () => {
			vi.useFakeTimers();
			const { host } = makeHost();
			try {
				await buildCleanerPlugin.onload(host);
				await vi.advanceTimersByTimeAsync(0);
				// The single startup scan seeds the bell/ticker for a user who never
				// opens the dashboard.
				expect(host.scanBuildArtifacts).toHaveBeenCalledOnce();

				await vi.advanceTimersByTimeAsync(24 * CADENCE);
				expect(host.scanBuildArtifacts).toHaveBeenCalledOnce();
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});

		it("polls on the configured cadence while the panel is open, and the panel consumes it", async () => {
			vi.useFakeTimers();
			const { host, panel, captured } = makeHost();
			try {
				await buildCleanerPlugin.onload(host);
				await vi.advanceTimersByTimeAsync(0);
				await captured.openDashboard?.();
				host.scanBuildArtifacts.mockClear();
				panel.update.mockClear();

				await vi.advanceTimersByTimeAsync(2 * CADENCE);
				expect(host.scanBuildArtifacts).toHaveBeenCalledTimes(2);
				// A walk nobody renders is the bug this story is about: each poll
				// must reach the open panel, not just the bell and ticker.
				expect(panel.update).toHaveBeenCalledTimes(2);
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});

		it("stops polling when the panel is hidden and resumes when it is shown", async () => {
			vi.useFakeTimers();
			const { host, captured } = makeHost();
			try {
				await buildCleanerPlugin.onload(host);
				await vi.advanceTimersByTimeAsync(0);
				await captured.openDashboard?.();

				captured.onVisibilityChange?.(false);
				host.scanBuildArtifacts.mockClear();
				await vi.advanceTimersByTimeAsync(3 * CADENCE);
				expect(host.scanBuildArtifacts).not.toHaveBeenCalled();

				captured.onVisibilityChange?.(true);
				await vi.advanceTimersByTimeAsync(CADENCE);
				expect(host.scanBuildArtifacts).toHaveBeenCalledOnce();
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});

		it("stops polling when the panel is closed", async () => {
			vi.useFakeTimers();
			const { host, captured } = makeHost();
			try {
				await buildCleanerPlugin.onload(host);
				await vi.advanceTimersByTimeAsync(0);
				await captured.openDashboard?.();

				captured.onClose?.();
				host.scanBuildArtifacts.mockClear();
				await vi.advanceTimersByTimeAsync(3 * CADENCE);
				expect(host.scanBuildArtifacts).not.toHaveBeenCalled();
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});

		it("does not resurrect the poll when the panel closes mid-save", async () => {
			vi.useFakeTimers();
			const { host, captured } = makeHost();
			// The real race: saveConfig is awaiting its write when the user closes
			// the tab. The host drops the message handler before firing onClose, so
			// the close lands inside the await, not before the message.
			let finishWrite: (() => void) | undefined;
			host.invoke.mockImplementation(async (command: string) => {
				if (command !== "write_plugin_data") return null;
				await new Promise<void>((resolve) => {
					finishWrite = resolve;
				});
				return null;
			});
			try {
				await buildCleanerPlugin.onload(host);
				await vi.advanceTimersByTimeAsync(0);
				await captured.openDashboard?.();

				const saving = captured.onMessage?.({ action: "saveConfig", config: configToForm(CFG) });
				await vi.advanceTimersByTimeAsync(0);
				captured.onClose?.();
				finishWrite?.();
				await saving;

				host.scanBuildArtifacts.mockClear();
				await vi.advanceTimersByTimeAsync(3 * CADENCE);
				expect(host.scanBuildArtifacts).not.toHaveBeenCalled();
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});

		it("leaves no timer running after onunload", async () => {
			vi.useFakeTimers();
			const { host, captured } = makeHost();
			try {
				await buildCleanerPlugin.onload(host);
				await vi.advanceTimersByTimeAsync(0);
				await captured.openDashboard?.();

				buildCleanerPlugin.onunload();
				host.scanBuildArtifacts.mockClear();
				await vi.advanceTimersByTimeAsync(5 * CADENCE);
				expect(host.scanBuildArtifacts).not.toHaveBeenCalled();
				expect(vi.getTimerCount()).toBe(0);
			} finally {
				buildCleanerPlugin.onunload();
				vi.useRealTimers();
			}
		});
	});
});
