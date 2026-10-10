// Fails if test infrastructure or tooling derives a path from `$HOME/Gits`.
//
// Test scratch used to live in, be pruned from, or be searched for under
// `~/Gits` (the upstream author's own layout). Wherever that directory cannot
// be written (an agent sandbox, a CI user, any other developer) `make check`
// aborted at its first step and several Rust test binaries hit EACCES. Scratch
// now comes from the caller's TMPDIR (scripts/with-test-tmp.sh,
// tuic-test-support, scripts/test-tmp-root.mjs); this guard keeps it that way.
//
// Scope: code and tooling only — scripts/, tools/, src-tauri/ (Rust, its
// scripts and nextest config), .github/, Makefile, vitest.config.ts,
// package.json. Docs are not scanned. Literal fixture strings such as a
// recorded `~/Gits/personal/...` status line in a Rust test are not
// filesystem access, so in Rust only HOME-derived spellings are flagged.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const selfTest = process.argv.includes("--self-test");

const REAL_ROOTS = ["scripts", "tools", "src-tauri", ".github", "Makefile", "vitest.config.ts", "package.json"];

const SKIP = [/^src-tauri\/(patches|icons|binaries|gen)\//, /\.(tcap|raw|png|jpe?g|icns|ico|wav|mp3|ogg|bin|lock|pdf|ttf|woff2?)$/i];

// Product/runtime references that deliberately keep `~/Gits` (assessment
// §1c). Each entry must still exist and still match, or the self-test fails.
const ALLOWED = new Map([
	["src-tauri/src/config.rs", "recognized_temp_roots: classifies historical ghost rows already in users' repositories.json"],
	["src-tauri/tauri.conf.json", "asset-protocol scope ($HOME/** skips dot-dirs); a product decision"],
	["scripts/check-no-home-gits.mjs", "this guard's own self-test fixtures"],
]);

const HOME_PATTERNS = [
	["shell/JSON $HOME/Gits", /\$\{?HOME\}?\/+Gits\b/],
	["Rust home_dir() + Gits", /home_dir\(\)[^;]*["']Gits/],
	["Rust env HOME + Gits", /env::var(?:_os)?\(\s*"HOME"\s*\)[\s\S]{0,200}?["'][^"'\n]*Gits/],
	["Rust Gits ancestor search", /==\s*"Gits"/],
	["Python Path.home() / Gits", /Path\.home\(\)\s*\/\s*["']Gits/],
	["Python HOME + Gits", /environ(?:\.get)?[[(]\s*["']HOME["'][\s\S]{0,200}?["'][^"'\n]*Gits/],
	["JS homedir() + Gits", /homedir\(\)[\s\S]{0,200}?["'`][^"'`\n]*Gits/],
	["JS env.HOME + Gits", /env\.HOME[\s\S]{0,200}?["'`][^"'`\n]*Gits/],
	["path.join(HOME, Gits)", /HOME\s*,\s*["']Gits/],
	["gitsRoot allow-list", /\bgitsRoot\b/],
];
// Tilde expansion only means $HOME in shell-ish tooling, not in Rust fixtures.
const TILDE_PATTERN = ["~/Gits path", /(?<![\w/])~\/Gits\b/];

function listTrackedFiles(roots, cwd) {
	const result = spawnSync("git", ["ls-files", "-z", "--", ...roots], { cwd, encoding: "utf8" });
	if (result.status !== 0) throw new Error(`git ls-files failed: ${result.stderr}`);
	return result.stdout.split("\0").filter(Boolean);
}

function lineOf(text, index) {
	return text.slice(0, index).split("\n").length;
}

/** @returns {{file: string, line: number, rule: string}[]} */
function scanFile(relPath, text) {
	const patterns = relPath.endsWith(".rs") ? HOME_PATTERNS : [...HOME_PATTERNS, TILDE_PATTERN];
	const hits = [];
	for (const [rule, pattern] of patterns) {
		const global = new RegExp(pattern.source, `${pattern.flags}g`);
		for (const match of text.matchAll(global)) hits.push({ file: relPath, line: lineOf(text, match.index), rule });
	}
	return hits;
}

function findOffenders(roots, cwd, allowed = ALLOWED) {
	const offenders = [];
	for (const relPath of listTrackedFiles(roots, cwd)) {
		if (SKIP.some((re) => re.test(relPath)) || allowed.has(relPath)) continue;
		const absPath = path.join(cwd, relPath);
		if (!fs.existsSync(absPath) || !fs.statSync(absPath).isFile()) continue;
		const buf = fs.readFileSync(absPath);
		if (buf.includes(0)) continue; // binary
		offenders.push(...scanFile(relPath, buf.toString("utf8")));
	}
	return offenders;
}

if (selfTest) {
	const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "tuic-home-gits-checker-"));
	const git = (...args) => spawnSync("git", args, { cwd: fixtureRoot, encoding: "utf8" });
	try {
		git("init", "-q");
		const offending = {
			"a.sh": 'mkdir -p "$HOME/Gits/.tmp"\n',
			"b.sh": "D=${HOME}/Gits/x\n",
			"c.sh": "cd ~/Gits/.tmp\n",
			"d.rs": 'let r = std::path::PathBuf::from(std::env::var("HOME").unwrap())\n    .join("Gits/.tmp");\n',
			"e.rs": 'let gits = dirs::home_dir().unwrap().join("Gits");\n',
			"f.rs": '.filter(|p| p.file_name().is_some_and(|n| n == "Gits"))\n',
			"g.py": 'root = Path.home() / "Gits" / ".tmp"\n',
			"h.mjs": 'const gitsRoot = path.join(process.env.HOME, "Gits");\n',
			"i.ts": 'const r = path.join(os.homedir(), "Gits", ".tmp");\n',
		};
		const clean = {
			"ok.rs": 'assert_eq!(clean("~/Gits/personal/tuicommander"), "");\n// the old `~/Gits` layout\n',
			"ok.sh": 'base="${TMPDIR:-/tmp}/tuic-tests"\n',
			"ok.py": 'root = Path(tempfile.gettempdir()) / "tuic-1419"\n',
		};
		for (const [name, text] of Object.entries({ ...offending, ...clean, "allowed.json": '"$HOME/Gits/**"\n' })) {
			fs.writeFileSync(path.join(fixtureRoot, name), text);
		}
		git("add", "-A");
		const found = findOffenders(["."], fixtureRoot, new Map([["allowed.json", "fixture"]]));
		const files = [...new Set(found.map((hit) => hit.file))].sort();
		const expected = Object.keys(offending).sort();
		if (JSON.stringify(files) !== JSON.stringify(expected)) {
			throw new Error(`expected offenders ${JSON.stringify(expected)}, got ${JSON.stringify(found)}`);
		}
		process.stdout.write(`home-Gits checker: ${expected.length} offending fixtures caught, clean and allow-listed fixtures pass\n`);

		// The fixture repo cannot see a stale allow-list or a shrunken scope.
		for (const [allowedPath, reason] of ALLOWED) {
			if (!fs.existsSync(allowedPath)) throw new Error(`allow-listed ${allowedPath} no longer exists (${reason})`);
			if (scanFile(allowedPath, fs.readFileSync(allowedPath, "utf8")).length === 0) {
				throw new Error(`allow-listed ${allowedPath} no longer mentions $HOME/Gits; drop it from ALLOWED`);
			}
		}
		const real = listTrackedFiles(REAL_ROOTS, process.cwd());
		for (const required of ["scripts/with-test-tmp.sh", "src-tauri/crates/tuic-test-support/src/lib.rs", "src-tauri/scripts/nextest-test-tmp.sh", "vitest.config.ts"]) {
			if (!real.includes(required)) throw new Error(`REAL_ROOTS no longer covers ${required}`);
		}
		process.stdout.write(`home-Gits checker: REAL_ROOTS covers ${real.length} tracked file(s); ${ALLOWED.size} allow-list entries are live\n`);
	} finally {
		fs.rmSync(fixtureRoot, { recursive: true, force: true });
	}
	process.exit(0);
}

const offenders = findOffenders(REAL_ROOTS, process.cwd());
if (offenders.length > 0) {
	process.stderr.write("Test infrastructure or tooling derives a path from $HOME/Gits:\n");
	for (const { file, line, rule } of offenders) process.stderr.write(`  ${file}:${line}  (${rule})\n`);
	process.stderr.write(
		"Use the caller's temp dir instead (TUIC_TEST_TMP_ROOT / TUIC_TEST_TMP_BASE / $TMPDIR, see scripts/with-test-tmp.sh).\n",
	);
	process.exit(1);
}
process.stdout.write("home-Gits checker: no $HOME/Gits paths in tracked code and tooling\n");
