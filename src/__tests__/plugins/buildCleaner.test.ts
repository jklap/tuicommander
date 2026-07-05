/**
 * Tests for the build-cleaner plugin's pure logic.
 *
 * Unlike claude-wakeup (whose logic is duplicated here because it had no
 * exports), build-cleaner exports its pure helpers as named exports alongside
 * the default plugin object, so we import and test the REAL implementation.
 * The plugin loader only reads `.default`, so these named exports are runtime-inert.
 */
import { describe, expect, it } from "vitest";
// @ts-expect-error — untyped plugin JS module (submodule), imported for its pure exports.
import * as plugin from "../../../plugins/build-cleaner/main.js";

const {
	fmtBytes,
	fmtAge,
	basename,
	relPath,
	evaluateThresholds,
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
		entries: Array<{ kind: string; size_bytes: number; last_modified_secs: number; repo: string; path: string }>,
		cfg: Record<string, unknown>,
		now: number,
	) => { severity: string; totalBytes: number; staleCount: number; largest: unknown };
	tickerPriority: (sev: string) => number;
	configToForm: (cfg: Record<string, unknown>) => Record<string, unknown>;
	formToConfig: (form: Record<string, unknown>, base: Record<string, unknown>) => Record<string, unknown>;
	buildPanelHtml: (entries: unknown[], cfg: Record<string, unknown>, now: number) => string;
};

const GIB = 1024 * 1024 * 1024;
const HOUR = 3600;
const NOW = 1_000_000_000; // fixed clock

const CFG = {
	perArtifactWarnBytes: 5 * GIB,
	totalWarnBytes: 50 * GIB,
	totalCriticalBytes: 150 * GIB,
	hotWindowSecs: 24 * HOUR,
	pollIntervalMs: 300_000,
	enabledKinds: ["rust", "node", "python", "dotnet", "gradle"],
};

/** An artifact built `ageHours` ago. */
function art(kind: string, gib: number, ageHours: number, repo = "/home/u/repoA", name = kind) {
	return {
		kind,
		size_bytes: gib * GIB,
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
		it("configToForm converts bytes/secs to GiB/hours", () => {
			const form = configToForm(CFG);
			expect(form.perArtifactWarnGiB).toBe(5);
			expect(form.totalWarnGiB).toBe(50);
			expect(form.hotWindowHours).toBe(24);
		});

		it("formToConfig is the inverse and clamps invalid values", () => {
			const form = configToForm(CFG);
			const back = formToConfig(form, CFG);
			expect(back.perArtifactWarnBytes).toBe(CFG.perArtifactWarnBytes);
			expect(back.totalWarnBytes).toBe(CFG.totalWarnBytes);
			expect(back.hotWindowSecs).toBe(CFG.hotWindowSecs);
		});

		it("formToConfig rejects non-positive numbers, falling back to base", () => {
			const back = formToConfig({ perArtifactWarnGiB: -3, totalWarnGiB: 0 }, CFG);
			expect(back.perArtifactWarnBytes).toBe(CFG.perArtifactWarnBytes);
			expect(back.totalWarnBytes).toBe(CFG.totalWarnBytes);
		});

		it("formToConfig drops unknown kinds and falls back if empty", () => {
			expect(formToConfig({ enabledKinds: ["rust", "bogus"] }, CFG).enabledKinds).toEqual(["rust"]);
			expect(formToConfig({ enabledKinds: ["bogus"] }, CFG).enabledKinds).toEqual([
				"rust",
				"node",
				"python",
				"dotnet",
				"gradle",
			]);
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

		it("escapes paths to prevent HTML injection", () => {
			const evil = art("rust", 1, 48, "/home/u/repoA", '"><script>alert(1)</script>');
			const html = buildPanelHtml([evil], CFG, NOW);
			expect(html).not.toContain("<script>alert(1)</script>");
			expect(html).toContain("&lt;script&gt;");
		});
	});
});
