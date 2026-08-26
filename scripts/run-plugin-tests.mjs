// Runs the plugins submodule's test suite and fails if it collects zero
// tests, not just if a test fails. `node --test "plugins/**/*.test.js"` on
// its own exits 0 when the glob matches nothing — `test:plugins` has been
// doing exactly that since it was added, because the `plugins` submodule
// (github.com/sstraus/tuicommander-plugins) currently has no test files at
// all. That made this CI step a permanent, silent no-op rather than a gate.
//
// This does not add tests to the submodule (out of scope here — it's a
// separate repository); it turns the vacuous pass into a visible failure so
// the gap is tracked instead of hidden. Once the submodule gains real tests,
// this starts asserting on them for free.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const selfTest = process.argv.includes("--self-test");

const PATTERN = "plugins/**/*.test.js";

/** Run `node --test` against `pattern` and classify the result.
 *  Returns {status: "fail"|"vacuous"|"pass", exitCode, stdout}. */
function runAndClassify(pattern, cwd) {
	const result = spawnSync(process.execPath, ["--test", "--test-reporter=tap", pattern], {
		cwd,
		encoding: "utf8",
		stdio: ["inherit", "pipe", "inherit"],
	});

	const testsMatch = result.stdout.match(/^# tests (\d+)$/m);
	const failMatch = result.stdout.match(/^# fail (\d+)$/m);
	const testCount = testsMatch ? Number(testsMatch[1]) : null;
	const failCount = failMatch ? Number(failMatch[1]) : null;

	if (testCount === null || failCount === null) {
		return { status: "unparseable", exitCode: 1, stdout: result.stdout };
	}
	if (result.status !== 0 && testCount > 0) {
		return { status: "fail", exitCode: result.status ?? 1, stdout: result.stdout };
	}
	if (testCount === 0) {
		return { status: "vacuous", exitCode: 1, stdout: result.stdout };
	}
	return { status: "pass", exitCode: 0, stdout: result.stdout };
}

if (selfTest) {
	const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "tuic-plugin-tests-"));
	try {
		// Branch 1: zero test files collected — the exact vacuous-pass bug this
		// script exists to fix. This is also what the real plugins submodule
		// hits today, so it's the only branch real CI actually exercises.
		const vacuous = runAndClassify("empty/**/*.test.js", fixtureRoot);
		if (vacuous.status !== "vacuous" || vacuous.exitCode === 0) {
			throw new Error(`expected a vacuous, failing result for zero matched files, got: ${JSON.stringify(vacuous)}`);
		}

		// Branch 2: a real, passing test file.
		fs.mkdirSync(path.join(fixtureRoot, "passing"), { recursive: true });
		fs.writeFileSync(
			path.join(fixtureRoot, "passing", "ok.test.js"),
			'import test from "node:test";\ntest("ok", () => {});\n',
		);
		const passing = runAndClassify("passing/**/*.test.js", fixtureRoot);
		if (passing.status !== "pass" || passing.exitCode !== 0) {
			throw new Error(`expected a passing result for a real passing test, got: ${JSON.stringify(passing)}`);
		}

		// Branch 3: a real, failing test file.
		fs.mkdirSync(path.join(fixtureRoot, "failing"), { recursive: true });
		fs.writeFileSync(
			path.join(fixtureRoot, "failing", "bad.test.js"),
			'import assert from "node:assert";\nimport test from "node:test";\ntest("bad", () => { assert.fail("boom"); });\n',
		);
		const failing = runAndClassify("failing/**/*.test.js", fixtureRoot);
		if (failing.status !== "fail" || failing.exitCode === 0) {
			throw new Error(`expected a failing result for a real failing test, got: ${JSON.stringify(failing)}`);
		}

		process.stdout.write(
			"run-plugin-tests: zero-match is caught as vacuous, a real pass passes, a real failure fails\n",
		);
	} finally {
		fs.rmSync(fixtureRoot, { recursive: true, force: true });
	}
	process.exit(0);
}

const result = runAndClassify(PATTERN, process.cwd());
process.stdout.write(result.stdout);

if (result.status === "unparseable") {
	process.stderr.write(`run-plugin-tests: could not parse a TAP summary out of the test runner's output for "${PATTERN}"\n`);
} else if (result.status === "vacuous") {
	process.stderr.write(
		`run-plugin-tests: 0 tests collected for "${PATTERN}" — the plugins submodule has no test files, ` +
			"so this gate has nothing to check. Add a test to the plugins submodule, or treat this as a known gap " +
			"rather than a passing suite.\n",
	);
}
process.exit(result.exitCode);
