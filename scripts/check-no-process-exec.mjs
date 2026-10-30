// Fails if Rust source (production OR tests) execs a process-inspection tool —
// `ps`, `pgrep`, `pkill` — or builds a shell command line that runs one,
// outside the allow-listed sites below.
//
// Why: `/bin/ps` is setuid root on macOS and sandboxed hosts refuse to exec
// it; `pgrep`/`pkill` need the `sysmond` mach service, which sandboxes do not
// expose. Every consumer that parsed their output went blind in exactly the
// environment TUIC most needs to see its children, and the test helpers that
// shelled out failed there for reasons unrelated to the code under test. The
// replacement is native: `tuic_core::process_info` (libproc / `kern.proc` on
// macOS, `/proc` on Linux). See plans/main-review/ps-exec-removal-assessment.md.
//
// Two rules, both on non-comment lines of tracked `.rs` files under
// `src-tauri/src` and `src-tauri/crates` (the vendored `src-tauri/patches` are
// not ours and are not scanned):
//   EXEC  — `Command::new("ps" | "pgrep" | "pkill")`, any path prefix
//           (`"/bin/ps"`), std or tokio.
//   SHELL — `ps -x` / `pgrep -x` / `pkill -x` inside text: an `sh -c` string,
//           a generated hook script, a test literal.
//
// The allow-list is keyed by `file::enclosing fn` (the nearest preceding `fn`
// line), so a new call in a different function of an allow-listed file is
// still caught. Every entry must still match something (`--self-test` checks
// the real tree), so a removed site cannot leave a stale exemption behind.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const selfTest = process.argv.includes("--self-test");

const REAL_ROOTS = ["src-tauri/src", "src-tauri/crates"];

const TOOLS = "ps|pgrep|pkill";
const EXEC_RE = new RegExp(String.raw`Command::new\(\s*"(?:[^"]*/)?(${TOOLS})"\s*\)`);
// A dash option (`ps -eo`, `pgrep -f`), or a BSD-style `ps` letter cluster
// with no dash (`ps aux`, `ps axo pid`, `ps ww`): only option letters, so prose
// such as "the ps fallback" is not a hit.
const SHELL_RE = new RegExp(
	String.raw`(?:^|[\s"'\x60(|;&$])(?:(${TOOLS})\s+-[A-Za-z]|(ps)\s+[aAcCeEhjlLmMrSTuvwxXo]{1,6}(?=$|[\s"'\x60;|&)\\]))`,
);
const COMMENT_RE = /^\s*(\/\/|\/\*|\*)/;
const FN_RE = /\bfn\s+([A-Za-z0-9_]+)/;
const INDENT_RE = /^(\s*)/;

/** `file::fn` -> why this site may keep its `ps` (and nothing else may). */
const ALLOWED = new Map([
	[
		"src-tauri/src/pty/screen.rs::ps_process_tree_snapshot",
		"the ONE production exec: logged fallback for the native snapshot when it fails or a sudo subtree's argv is unreadable (user decisions Q1/Q5; remove once the native path has soaked)",
	],
	[
		"src-tauri/src/agent_hook.rs::tty_resolve",
		"headless hook shell text run by the AGENT: its live parent tty wins over an inherited $TUIC_PTY_TTY, which is the fallback when this ps cannot run (sandbox) or the parent is detached; TUIC itself execs nothing",
	],
	[
		"src-tauri/src/agent_hook_launch.rs::codex_script",
		"Codex notify script run by the AGENT, same live-tty-then-$TUIC_PTY_TTY order as tty_resolve",
	],
	[
		"src-tauri/src/remote_deploy/mod.rs::verified_kill",
		"shell text executed on the REMOTE host over ssh, not under the local sandbox",
	],
	[
		"src-tauri/src/remote_deploy/mod.rs::host_ps_reports_comm",
		"test probe: decides whether verified_kill's remote-side `ps` can run locally; when it cannot (sandbox), the test puts tuic-test-support's native `ps` stand-in on PATH instead, so nothing depends on /bin/ps",
	],
	[
		"src-tauri/crates/tuic-core/src/process_info.rs::native_snapshot_agrees_with_ps",
		"macOS test: compares the native table against the real /bin/ps where it can run, and reports UNCHECKED where it cannot (the sandbox); it is the oracle the native module replaced",
	],
	[
		"src-tauri/src/agent_hook.rs::shell_command_resolves_the_tty_and_prints_every_wire_pair",
		"test asserts on the generated fallback text; execs nothing",
	],
	[
		"src-tauri/src/agent_hook.rs::shell_command_always_orders_toolfail_before_state_regardless_of_input_order",
		"test asserts on the generated fallback text; execs nothing",
	],
	[
		"src-tauri/src/agent_hook.rs::every_command_invokes_the_tuic_hook_binary_only_if_executable",
		"test asserts the binary flavour does NOT contain the fallback text",
	],
	[
		"src-tauri/src/agent_hook_launch.rs::generated_assets_have_protocol_and_ownership_markers",
		"test asserts on the generated fallback text; execs nothing",
	],
]);

function listTrackedRustFiles(roots, cwd) {
	const result = spawnSync("git", ["ls-files", "-z", "--", ...roots], { cwd, encoding: "utf8" });
	if (result.status !== 0) {
		throw new Error(`git ls-files failed: ${result.stderr}`);
	}
	return result.stdout.split("\0").filter((file) => file.endsWith(".rs"));
}

/**
 * @param {{path: string, text: string}[]} files
 * @returns {{hits: {site: string, line: number, rule: string, text: string}[]}}
 */
function scan(files) {
	const hits = [];
	for (const { path: file, text } of files) {
		// Open function bodies, innermost last. The tree is rustfmt-formatted,
		// so a body ends at the first `}` line back at its `fn` line's indent:
		// a const, static or test literal after it is no longer "inside" that
		// fn (it used to inherit the last allow-listed name it followed).
		const open = [];
		const lines = text.split("\n");
		for (let i = 0; i < lines.length; i++) {
			const line = lines[i];
			if (line.trim() === "") continue;
			const indent = INDENT_RE.exec(line)[1].length;
			while (open.length > 0) {
				const top = open[open.length - 1];
				if (indent < top.indent) {
					open.pop();
				} else if (indent === top.indent && /^\s*}/.test(line)) {
					open.pop();
					break;
				} else {
					break;
				}
			}
			if (COMMENT_RE.test(line)) continue;
			const fn = FN_RE.exec(line);
			const enclosing = fn ? fn[1] : open.length > 0 ? open[open.length - 1].name : "<top level>";
			// A body that stays open past this line: not a `;` declaration, not
			// a one-line `fn f() { .. }`.
			if (fn && !/[;}]\s*$/.test(line)) open.push({ name: fn[1], indent });
			const rule = EXEC_RE.test(line) ? "EXEC" : SHELL_RE.test(line) ? "SHELL" : null;
			if (rule) hits.push({ site: `${file}::${enclosing}`, line: i + 1, rule, text: line.trim() });
		}
	}
	return { hits };
}

function offendersOf(hits, allowed) {
	return hits.filter((hit) => !allowed.has(hit.site));
}

function readTree(cwd) {
	return listTrackedRustFiles(REAL_ROOTS, cwd)
		.map((file) => ({ file, abs: path.join(cwd, file) }))
		.filter(({ abs }) => fs.existsSync(abs))
		.map(({ file, abs }) => ({ path: file, text: fs.readFileSync(abs, "utf8") }));
}

if (selfTest) {
	const fixtures = [
		{
			path: "a.rs",
			text: 'fn probe() {\n    let out = std::process::Command::new("ps").arg("-ax").output();\n}\n',
		},
		{ path: "b.rs", text: 'fn kids() {\n    tokio::process::Command::new("/usr/bin/pgrep");\n}\n' },
		{ path: "c.rs", text: 'fn shelled() {\n    run("sh", "-c", "ps -eo pid,ppid | awk 1");\n}\n' },
		{ path: "d.rs", text: "fn documented() {\n    // ps -o tty= used to be here\n    let ps = 3; let x = ps - 1;\n}\n" },
		{ path: "e.rs", text: 'fn allowed_site() {\n    Command::new("ps");\n}\nfn other() {\n    Command::new("pkill");\n}\n' },
		// A top-level const after an allow-listed fn is not inside it.
		{
			path: "f.rs",
			text: 'fn allowed_site() {\n    Command::new("ps");\n}\n\nconst SNEAKY: &str = "ps -eo pid";\n',
		},
		// Same for a module-level const after a nested allow-listed fn, and a
		// multi-line signature stays inside its fn.
		{
			path: "g.rs",
			text:
				'mod tests {\n    fn allowed_site(\n        a: u8,\n    ) -> u8 {\n        run("ps -o comm=");\n        a\n    }\n    const LATER: &str = "pgrep -f x";\n}\n',
		},
		// BSD-style letters with no dash, and prose that must not match.
		{ path: "h.rs", text: 'fn bsd() {\n    run("sh", "-c", "ps aux | grep x");\n    log("the ps fallback ran");\n}\n' },
		// A one-line fn and a `;` declaration do not swallow what follows.
		{
			path: "i.rs",
			text: 'trait T {\n    fn allowed_site(&self);\n}\nfn allowed_one() -> u8 { 1 }\nstatic S: &str = "ps -A";\n',
		},
	];
	const allowed = new Map([
		["e.rs::allowed_site", "fixture"],
		["f.rs::allowed_site", "fixture"],
		["g.rs::allowed_site", "fixture"],
		["i.rs::allowed_site", "fixture"],
		["i.rs::allowed_one", "fixture"],
	]);
	const { hits } = scan(fixtures);
	const sites = offendersOf(hits, allowed).map((hit) => `${hit.site}:${hit.rule}`);
	const expected = [
		"a.rs::probe:EXEC",
		"b.rs::kids:EXEC",
		"c.rs::shelled:SHELL",
		"e.rs::other:EXEC",
		"f.rs::<top level>:SHELL",
		"g.rs::<top level>:SHELL",
		"h.rs::bsd:SHELL",
		"i.rs::<top level>:SHELL",
	];
	if (JSON.stringify(sites) !== JSON.stringify(expected)) {
		throw new Error(`expected offenders ${JSON.stringify(expected)}, got ${JSON.stringify(sites)}`);
	}
	process.stdout.write(
		"process-exec checker: exec/shell/BSD-style/path-prefixed calls caught, comments, prose and arithmetic ignored, allow-list is per function body\n",
	);

	// The fixture run above would stay green if REAL_ROOTS stopped covering the
	// crates or an allow-list entry went stale; check both against the real tree.
	const real = readTree(process.cwd());
	if (!real.some((file) => file.path.startsWith("src-tauri/crates/"))) {
		throw new Error("REAL_ROOTS found zero Rust files under src-tauri/crates/");
	}
	const realHits = scan(real).hits;
	const stale = [...ALLOWED.keys()].filter((site) => !realHits.some((hit) => hit.site === site));
	if (stale.length > 0) {
		throw new Error(`allow-list entries that no longer match anything (remove them): ${stale.join(", ")}`);
	}
	process.stdout.write(`process-exec checker: all ${ALLOWED.size} allow-list entries still match a real site\n`);
	process.exit(0);
}

const offenders = offendersOf(scan(readTree(process.cwd())).hits, ALLOWED);
if (offenders.length > 0) {
	process.stderr.write("Rust source execs (or scripts) a process-inspection tool outside the allow-list:\n");
	for (const hit of offenders) process.stderr.write(`  ${hit.site} (line ${hit.line}, ${hit.rule}): ${hit.text}\n`);
	process.stderr.write(
		"Use tuic_core::process_info (native libproc / /proc) instead — sandboxed hosts cannot exec ps/pgrep/pkill.\n" +
			"If a site genuinely must keep one, add it to ALLOWED in scripts/check-no-process-exec.mjs with the reason.\n",
	);
	process.exit(1);
}
process.stdout.write("process-exec checker: no ps/pgrep/pkill exec outside the allow-list\n");
