// Fails if any tracked source file contains a literal NUL byte. `ugrep -I`
// (what Claude Code's `grep` wrapper runs, and what several editors/linters
// use to skip binary files) treats a file with an embedded NUL as binary and
// silently returns nothing for it — so a stray NUL doesn't just look odd, it
// blinds every grep-based audit of that file. `CanvasTerminal.tsx` shipped
// with two for weeks before anyone noticed. Source code should express a NUL
// as the `\u0000`/`\x00` escape, which is byte-identical at runtime and keeps
// the file readable as text.
//
// Scoped to source-code extensions only — `src/` and `src-tauri/src/` also
// hold legitimate binary assets (icons, `no-tui-open.png`) that are expected
// to contain NUL bytes.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const selfTest = process.argv.includes("--self-test");

const SOURCE_EXTENSIONS = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".rs"];

// `src-tauri/src` alone misses the OTHER three Cargo workspace crates under
// `src-tauri/crates/*/src` (tuic-hook, tuic-cli, tuic-bridge) — found missing
// during a review of the tuic-hook work this checker was meant to protect.
const REAL_ROOTS = ["src", "src-tauri/src", "src-tauri/crates"];

function listTrackedFiles(roots, cwd) {
	const result = spawnSync("git", ["ls-files", "-z", "--", ...roots], { cwd, encoding: "utf8" });
	if (result.status !== 0) {
		throw new Error(`git ls-files failed: ${result.stderr}`);
	}
	return result.stdout.split("\0").filter(Boolean);
}

function findNulFiles(roots, cwd) {
	const offenders = [];
	for (const relPath of listTrackedFiles(roots, cwd)) {
		if (!SOURCE_EXTENSIONS.includes(path.extname(relPath))) continue;
		const absPath = path.join(cwd, relPath);
		if (!fs.existsSync(absPath)) continue; // deleted-but-still-staged, etc.
		const buf = fs.readFileSync(absPath);
		if (buf.includes(0)) offenders.push(relPath);
	}
	return offenders;
}

if (selfTest) {
	const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "tuic-nul-checker-"));
	try {
		spawnSync("git", ["init", "-q"], { cwd: fixtureRoot });
		spawnSync("git", ["config", "user.email", "test@example.com"], { cwd: fixtureRoot });
		spawnSync("git", ["config", "user.name", "test"], { cwd: fixtureRoot });

		fs.writeFileSync(path.join(fixtureRoot, "clean.ts"), 'export const key = "\\u0000";\n');
		spawnSync("git", ["add", "-A"], { cwd: fixtureRoot });
		const clean = findNulFiles(["."], fixtureRoot);
		if (clean.length !== 0) {
			throw new Error(`expected no offenders for the escaped-NUL fixture, got: ${clean.join(", ")}`);
		}

		fs.writeFileSync(path.join(fixtureRoot, "dirty.ts"), Buffer.from("export const key = `a\0b`;\n"));
		spawnSync("git", ["add", "-A"], { cwd: fixtureRoot });
		const dirty = findNulFiles(["."], fixtureRoot);
		if (dirty.length !== 1 || dirty[0] !== "dirty.ts") {
			throw new Error(`expected exactly ["dirty.ts"], got: ${JSON.stringify(dirty)}`);
		}

		fs.writeFileSync(path.join(fixtureRoot, "binary.png"), Buffer.from([0x89, 0x50, 0x4e, 0x47, 0]));
		spawnSync("git", ["add", "-A"], { cwd: fixtureRoot });
		const withBinary = findNulFiles(["."], fixtureRoot);
		if (withBinary.length !== 1 || withBinary[0] !== "dirty.ts") {
			throw new Error(`binary asset should be skipped by extension, got: ${JSON.stringify(withBinary)}`);
		}

		process.stdout.write("NUL-byte checker: clean fixture passes, literal-NUL fixture is caught, binary assets are skipped\n");

		// The generic-filter assertions above run against a throwaway fixture repo,
		// so they'd stay green even if REAL_ROOTS regressed back to missing
		// src-tauri/crates entirely — check the real constant against the real repo.
		const realCrateFiles = listTrackedFiles(REAL_ROOTS, process.cwd()).filter((f) =>
			f.startsWith("src-tauri/crates/"),
		);
		if (realCrateFiles.length === 0) {
			throw new Error(
				"REAL_ROOTS found zero files under src-tauri/crates/ in the real repo — " +
					"the other three Cargo workspace crates (tuic-hook, tuic-cli, tuic-bridge) would be unscanned",
			);
		}
		process.stdout.write(`NUL-byte checker: REAL_ROOTS covers ${realCrateFiles.length} file(s) under src-tauri/crates/\n`);
	} finally {
		fs.rmSync(fixtureRoot, { recursive: true, force: true });
	}
	process.exit(0);
}

const offenders = findNulFiles(REAL_ROOTS, process.cwd());
if (offenders.length > 0) {
	process.stderr.write("Found literal NUL byte(s) in tracked source file(s):\n");
	for (const file of offenders) process.stderr.write(`  ${file}\n`);
	process.stderr.write("Replace the raw NUL with a \\u0000 / \\x00 escape — same runtime value, readable as text.\n");
	process.exit(1);
}
process.stdout.write(`NUL-byte checker: ${offenders.length} offenders in tracked src/**, src-tauri/src/**, src-tauri/crates/** source files\n`);
