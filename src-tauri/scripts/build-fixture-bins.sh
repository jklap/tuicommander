#!/bin/sh
# `cargo nextest run --lib` builds only the lib target, but several lib unit
# tests spawn the package's [[bin]] fixtures (tuic-acp-fixture-agent,
# tuic-mcp-fixture-server) as subprocesses. Build them once, here, before the
# run starts: a per-test on-demand build races under nextest, which runs one
# process per test (ten nested `cargo build`s competing for the same lock).
#
# The headless default still exercises the exact binaries lib tests spawn:
# neither fixture is feature-gated. It avoids desktop permission validation
# for local lib-only runs. CI supplies TUIC_FIXTURE_FEATURES to reuse its graph.
# Match nextest's Cargo test profile, not dev: test disables debug info and
# has separate optimization settings. CI also passes its feature set so setup
# can reuse the artifacts Cargo built for the integration tests.
set -eu
set -- --no-default-features
if [ -n "${TUIC_FIXTURE_FEATURES:-}" ]; then
    set -- "$@" --features "$TUIC_FIXTURE_FEATURES"
fi
cargo build --profile test "$@" --bin tuic-acp-fixture-agent --bin tuic-mcp-fixture-server
