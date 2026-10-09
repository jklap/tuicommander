#!/usr/bin/env bash
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
test_tmp="${TUIC_TEST_TMP_ROOT:-$project_root/.tmp/tuic-tests}"
mkdir -p "$test_tmp"
scratch="$(mktemp -d "$test_tmp/fixture-gate.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

git -C "$scratch" init -q repo
repo="$scratch/repo"
mkdir -p "$repo/src-tauri/src"
printf 'fn awaiting_input() { false }\n' > "$repo/src-tauri/src/pty.rs"
git -C "$repo" add src-tauri/src/pty.rs
git -C "$repo" -c user.name=Test -c user.email=test@example.com commit -qm initial

printf 'fn awaiting_input() { true }\n' > "$repo/src-tauri/src/pty.rs"
git -C "$repo" add src-tauri/src/pty.rs
if (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/blocked" 2>&1; then
  echo 'detection change unexpectedly passed without a fixture' >&2
  exit 1
fi
grep -Fq 'agent-state detection changed with no capture' "$scratch/blocked"
grep -Fq 'The output ring holds 2 MB' "$scratch/blocked"
grep -Fq '8192-byte output page limit' "$scratch/blocked"
if grep -Fq 'ring holds 8 KB' "$scratch/blocked"; then
  echo 'hook still reports the output page size as ring retention' >&2
  exit 1
fi

# Catches blanket fixture-format policing: raw/text additions alone are not production edits.
git -C "$repo" restore --staged src-tauri/src/pty.rs
mkdir -p "$repo/src-tauri/src/fixtures/agent_prompts"
printf 'hook-only fixture content\n' > "$repo/src-tauri/src/fixtures/agent_prompts/unrelated.txt"
git -C "$repo" add src-tauri/src/fixtures/agent_prompts/unrelated.txt
(cd "$repo" && bash "$project_root/scripts/hooks/pre-commit")
git -C "$repo" rm -f src-tauri/src/fixtures/agent_prompts/unrelated.txt
git -C "$repo" add src-tauri/src/pty.rs

# A handwritten external fixture must not unlock a production detection edit.
mkdir -p "$repo/src-tauri/src/fixtures/agent_prompts"
printf 'invented agent approval output\n' > "$repo/src-tauri/src/fixtures/agent_prompts/invented.raw"
git -C "$repo" add src-tauri/src/fixtures/agent_prompts/invented.raw
if (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/handwritten" 2>&1; then
  echo 'handwritten external fixture unexpectedly satisfied the capture gate' >&2
  exit 1
fi

# Remove only this test-created staged fake before exercising the valid capture case.
git -C "$repo" rm -f src-tauri/src/fixtures/agent_prompts/invented.raw
mkdir -p "$repo/src-tauri/src/fixtures/agent_prompts"

# Use a genuinely recorded fixture already tracked by this repository, never fake TCAP bytes.
capture='src-tauri/src/fixtures/agent_prompts/claude-askuser-esc-20260929'
cp "$project_root/$capture.tcap" "$repo/$capture.tcap"
cp "$project_root/$capture.md" "$repo/$capture.md"
git -C "$repo" add "$capture.tcap" "$capture.md"
(cd "$repo" && bash "$project_root/scripts/hooks/pre-commit")
git -C "$repo" -c user.name=Test -c user.email=test@example.com commit -qm recorded-baseline

cat > "$repo/src-tauri/src/state.rs" <<'RS'
fn question_text() { false }
#[cfg(test)]
mod tests {
    #[test]
    fn existing() {
        // A comment ending in a quote must not open a string: "question"
        let recorded = r###"literal with } { and # [cfg(test)]"###;
        assert!(!recorded.is_empty());
    }
}
RS
git -C "$repo" add src-tauri/src/state.rs
git -C "$repo" -c user.name=Test -c user.email=test@example.com commit -qm state-baseline

# Catches the false positive: symbols in cfg(test) additions using an existing capture.
python3 - "$repo/src-tauri/src/state.rs" <<'PYTEST'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
source = source.replace("    #[test]", """    #[test]
    fn question_text_from_existing_capture() {
        assert!(!include_bytes!("fixtures/agent_prompts/claude-askuser-esc-20260929.tcap").is_empty());
        let question_text = "awaiting_input";
        assert!(!question_text.is_empty());
    }
    #[test]""", 1)
path.write_text(source)
PYTEST
git -C "$repo" add src-tauri/src/state.rs
(cd "$repo" && bash "$project_root/scripts/hooks/pre-commit")

# Standalone test functions and removals are also test-only, including multiline literals.
cat >> "$repo/src-tauri/src/state.rs" <<'RS'
#[test]
fn question_cleared() {
    let question_text = "braces } {
inside a literal";
    assert!(!question_text.is_empty());
}
RS
git -C "$repo" add src-tauri/src/state.rs
(cd "$repo" && bash "$project_root/scripts/hooks/pre-commit")

# Catches an overbroad exemption: one production line alongside tests must still block.
sed 's/fn question_text() { false }/fn question_text() { true }/' "$repo/src-tauri/src/state.rs" > "$scratch/state"
cp "$scratch/state" "$repo/src-tauri/src/state.rs"
git -C "$repo" add src-tauri/src/state.rs
if (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/mixed" 2>&1; then
  echo 'production detection change mixed with tests unexpectedly passed' >&2
  exit 1
fi

# Catches index/working-tree confusion: restoring only the working tree cannot hide staged logic.
sed 's/fn question_text() { true }/fn question_text() { false }/' "$repo/src-tauri/src/state.rs" > "$scratch/state"
cp "$scratch/state" "$repo/src-tauri/src/state.rs"
if (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/index" 2>&1; then
  echo 'unstaged restoration hid a staged production change' >&2
  exit 1
fi
# Drop this test's own staged state.rs edit before the next scenario.
git -C "$repo" checkout -q HEAD -- src-tauri/src/state.rs

# A test-module file whose function forgot its `#[test]` attribute: the function is
# production-shaped in HEAD, a test in the index. Catches the false positive where
# ADDING the attribute read as deleting production code that mentions 7770.
mkdir -p "$repo/src-tauri/src/pty"
cat > "$repo/src-tauri/src/pty/tests.rs" <<'RS'
fn awaiting_input() -> bool {
    false
}

#[test]
fn existing_osc_test() {
    assert!(!awaiting_input());
}

fn heuristic_without_osc_7770() {
    let seq = 7770;
    assert_eq!(seq, 7770);
}
RS
git -C "$repo" add src-tauri/src/pty/tests.rs
git -C "$repo" -c user.name=Test -c user.email=test@example.com commit -qm untagged-test-baseline
attribute_only() {
  python3 - "$repo/src-tauri/src/pty/tests.rs" <<'PYTEST'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
path.write_text(source.replace("fn heuristic_without_osc_7770", "#[test]\nfn heuristic_without_osc_7770", 1))
PYTEST
}
attribute_only
git -C "$repo" add src-tauri/src/pty/tests.rs
if ! (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/attribute" 2>&1; then
  cat "$scratch/attribute" >&2
  echo 'adding #[test] to an attribute-less test function was blocked as a production change' >&2
  exit 1
fi

# Catches an overbroad exemption: adding the attribute must not hide a real production
# detection edit staged in the same file.
sed 's/^    false$/    true/' "$repo/src-tauri/src/pty/tests.rs" > "$scratch/pty-tests"
cp "$scratch/pty-tests" "$repo/src-tauri/src/pty/tests.rs"
git -C "$repo" add src-tauri/src/pty/tests.rs
if (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/attribute-mixed" 2>&1; then
  echo 'a production detection edit next to an added #[test] unexpectedly passed' >&2
  exit 1
fi

# Catches an overbroad exemption: turning a function into a test while also editing its
# detection logic is not an attribute-only change, so the old production body still counts.
git -C "$repo" checkout -q HEAD -- src-tauri/src/pty/tests.rs
attribute_only
sed 's/let seq = 7770;/let seq = 7771;/' "$repo/src-tauri/src/pty/tests.rs" > "$scratch/pty-tests"
cp "$scratch/pty-tests" "$repo/src-tauri/src/pty/tests.rs"
git -C "$repo" add src-tauri/src/pty/tests.rs
if (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/attribute-edit" 2>&1; then
  echo 'converting a detection function into an edited test unexpectedly passed' >&2
  exit 1
fi

# Catches an overbroad exemption: a NEW test that is a verbatim copy of a production
# detection function must not let that production function's removal slip through.
git -C "$repo" checkout -q HEAD -- src-tauri/src/pty/tests.rs
python3 - "$repo/src-tauri/src/pty/tests.rs" <<'PYTEST'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
body = "fn heuristic_without_osc_7770() {\n    let seq = 7770;\n    assert_eq!(seq, 7770);\n}\n"
# Remove the production copy, and add the identical text as a test inside a test module.
source = source.replace(body, "")
source += "#[cfg(test)]\nmod copied {\n#[test]\n" + body + "}\n"
path.write_text(source)
PYTEST
git -C "$repo" add src-tauri/src/pty/tests.rs
if ! (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/attribute-move" 2>&1; then
  # Moving the function verbatim into a test module is the same attribute-only change.
  cat "$scratch/attribute-move" >&2
  echo 'moving an attribute-less test function verbatim into a test module was blocked' >&2
  exit 1
fi
# Catches double counting: keeping the attribute-less function AND adding an identical
# test copy is test-only; the kept function must stay in both views (exempting it from
# HEAD alone would read as a production insertion and falsely block).
git -C "$repo" checkout -q HEAD -- src-tauri/src/pty/tests.rs
python3 - "$repo/src-tauri/src/pty/tests.rs" <<'PYTEST'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
source = path.read_text()
body = "fn heuristic_without_osc_7770() {\n    let seq = 7770;\n    assert_eq!(seq, 7770);\n}\n"
source += "#[cfg(test)]\nmod copied {\n#[test]\n" + body + "}\n"
path.write_text(source)
PYTEST
git -C "$repo" add src-tauri/src/pty/tests.rs
if ! (cd "$repo" && bash "$project_root/scripts/hooks/pre-commit") > "$scratch/attribute-copy" 2>&1; then
  cat "$scratch/attribute-copy" >&2
  echo 'adding a test copy next to an unchanged function was blocked' >&2
  exit 1
fi
echo 'fixture gate regressions passed'
