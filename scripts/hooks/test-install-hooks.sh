#!/usr/bin/env bash
set -euo pipefail

project_root="$(git rev-parse --show-toplevel)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

git -C "$scratch" init -q repo
repo="$scratch/repo"
git -C "$repo" -c core.hooksPath=/dev/null -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m initial
git -C "$repo" worktree add -q --detach "$scratch/other"

for checkout in "$repo" "$scratch/other"; do
  checkout="$(git -C "$checkout" rev-parse --show-toplevel)"
  mkdir -p "$checkout/scripts/hooks"
  cp "$project_root/scripts/hooks/install-hooks.sh" "$checkout/scripts/hooks/install-hooks.sh"
  cp "$project_root/scripts/hooks/test-install-hooks.sh" "$checkout/scripts/hooks/test-install-hooks.sh"
  printf '#!/bin/sh\nprintf "%%s\\n" "%s" >> "$HOOK_RESULTS"\n' "$checkout" > "$checkout/scripts/hooks/pre-commit"
  chmod +x "$checkout/scripts/hooks/pre-commit"
done

printf '#!/bin/sh\n# external hook\n' > "$repo/.git/hooks/pre-push"
cp "$repo/scripts/hooks/pre-commit" "$repo/scripts/hooks/pre-push"

(cd "$repo" && bash scripts/hooks/install-hooks.sh)
(cd "$scratch/other" && bash scripts/hooks/install-hooks.sh)
grep -Fqx '# external hook' "$repo/.git/hooks/pre-push"
# This test lives beside the hooks but is not one.
[ ! -e "$repo/.git/hooks/test-install-hooks.sh" ]

: > "$scratch/results"
HOOK_RESULTS="$scratch/results" git -C "$repo" -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m main
HOOK_RESULTS="$scratch/results" git -C "$scratch/other" -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m other
printf '%s\n%s\n' "$(git -C "$repo" rev-parse --show-toplevel)" "$(git -C "$scratch/other" rev-parse --show-toplevel)" > "$scratch/expected"
diff -u "$scratch/expected" "$scratch/results"

# Reinstalling one checkout must not redirect hooks in the other.
(cd "$repo" && bash scripts/hooks/install-hooks.sh)
: > "$scratch/results"
HOOK_RESULTS="$scratch/results" git -C "$scratch/other" -c user.name=Test -c user.email=test@example.com commit -q --allow-empty -m again
git -C "$scratch/other" rev-parse --show-toplevel > "$scratch/expected"
diff -u "$scratch/expected" "$scratch/results"
