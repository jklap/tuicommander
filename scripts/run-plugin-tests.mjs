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

const PATTERN = "plugins/**/*.test.js";

const result = spawnSync(process.execPath, ["--test", "--test-reporter=tap", PATTERN], {
	encoding: "utf8",
	stdio: ["inherit", "pipe", "inherit"],
});

process.stdout.write(result.stdout);

const testsMatch = result.stdout.match(/^# tests (\d+)$/m);
const failMatch = result.stdout.match(/^# fail (\d+)$/m);
const testCount = testsMatch ? Number(testsMatch[1]) : null;
const failCount = failMatch ? Number(failMatch[1]) : null;

if (testCount === null || failCount === null) {
	process.stderr.write(`run-plugin-tests: could not parse a TAP summary out of the test runner's output for "${PATTERN}"\n`);
	process.exit(1);
}

if (result.status !== 0 && testCount > 0) {
	// A real failure among real tests — the test runner's own exit code is authoritative.
	process.exit(result.status ?? 1);
}

if (testCount === 0) {
	process.stderr.write(
		`run-plugin-tests: 0 tests collected for "${PATTERN}" — the plugins submodule has no test files, ` +
			"so this gate has nothing to check. Add a test to the plugins submodule, or treat this as a known gap " +
			"rather than a passing suite.\n",
	);
	process.exit(1);
}

process.exit(0);
