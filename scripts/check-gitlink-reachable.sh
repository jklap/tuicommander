#!/bin/sh
# Fail when a submodule gitlink pinned in HEAD is not an ancestor of the default
# branch of the submodule remote (an unpushed pin breaks every fresh checkout).
# Usage: check-gitlink-reachable.sh [repo-dir] [submodule-path]
set -eu

repo=${1:-.}
path=${2:-plugins}

pinned=$(git -C "$repo" ls-tree HEAD -- "$path" | awk '$1 == "160000" { print $3 }')
[ -n "$pinned" ] || { echo "no gitlink at '$path' in HEAD" >&2; exit 2; }

name=$(git config -f "$repo/.gitmodules" --get-regexp '^submodule\..*\.path$' |
  awk -v p="$path" '$2 == p { sub(/^submodule\./, "", $1); sub(/\.path$/, "", $1); print $1 }')
url=$(git config -f "$repo/.gitmodules" --get "submodule.$name.url")
[ -n "$url" ] || { echo "no url for submodule '$path' in .gitmodules" >&2; exit 2; }

work=$(mktemp -d "${TMPDIR:-/tmp}/gitlink.XXXXXX")
trap 'rm -rf "$work"' EXIT
git init -q "$work"
git -C "$work" remote add origin "$url"
git -C "$work" fetch -q origin
git -C "$work" remote set-head origin -a >/dev/null
default=$(git -C "$work" symbolic-ref --short refs/remotes/origin/HEAD)

if git -C "$work" merge-base --is-ancestor "$pinned" "$default" 2>/dev/null; then
  echo "ok: $path pin $pinned is on $default of $url"
else
  echo "FAIL: $path pin $pinned is not on $default of $url (push the submodule first)" >&2
  exit 1
fi
