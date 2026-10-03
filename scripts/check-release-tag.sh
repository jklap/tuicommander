#!/usr/bin/env bash
# Fails when a release tag does not match the version the bump wrote into the
# manifests. release.yml derives the release name from tauri.conf.json, so a tag
# pushed before `make bump` would overwrite the assets of the previous release.
#
#   scripts/check-release-tag.sh <tag> [repo-root]   e.g. v1.8.0
#   scripts/check-release-tag.sh --self-test
set -euo pipefail

check() {
  local tag=$1 root=$2 want=${1#v} bad=0 got file
  for file in src-tauri/tauri.conf.json package.json; do
    got=$(jq -r '.version' "$root/$file")
    [[ "$got" == "$want" ]] || { echo "::error::$file is $got but the tag is $tag" >&2; bad=1; }
  done
  got=$(awk '/^\[workspace.package\]/{on=1;next} /^\[/{on=0} on && /^version/{gsub(/"/,"",$3);print $3;exit}' "$root/src-tauri/Cargo.toml")
  [[ "$got" == "$want" ]] || { echo "::error::src-tauri/Cargo.toml [workspace.package] is $got but the tag is $tag" >&2; bad=1; }
  return $bad
}

self_test() {
  dir=$(mktemp -d "${TMPDIR:-/tmp}/tuic-release-tag.XXXXXX")
  trap 'rm -rf "$dir"' EXIT
  mkdir -p "$dir/src-tauri"
  printf '{"version": "1.7.7"}\n' > "$dir/src-tauri/tauri.conf.json"
  printf '{\n  "version": "1.7.7"\n}\n' > "$dir/package.json"
  printf '[workspace.package]\nversion = "1.7.7"\n\n[package]\nversion.workspace = true\n' > "$dir/src-tauri/Cargo.toml"
  check v1.7.7 "$dir" || { echo "self-test: matching tag rejected" >&2; exit 1; }
  # Catches: a v1.8.0 tag pushed before the bump, which overwrites the 1.7.7 release.
  ! check v1.8.0 "$dir" 2>/dev/null || { echo "self-test: unbumped tag accepted" >&2; exit 1; }
  # Catches: a bump that updated tauri.conf.json but missed one other manifest.
  printf '{"version": "1.8.0"}\n' > "$dir/src-tauri/tauri.conf.json"
  printf '{"version": "1.8.0"}\n' > "$dir/package.json"
  ! check v1.8.0 "$dir" 2>/dev/null || { echo "self-test: stale Cargo.toml accepted" >&2; exit 1; }
  echo "check-release-tag self-test ok"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
else
  check "${1:?usage: check-release-tag.sh <tag> [repo-root]}" "${2:-.}"
fi
