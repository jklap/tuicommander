#!/bin/sh
# `cargo nextest run --lib` builds only the lib target, but several lib unit
# tests spawn the package's [[bin]] fixtures (tuic-acp-fixture-agent,
# tuic-mcp-fixture-server) as subprocesses. Build them once, here, before the
# run starts: a per-test on-demand build races under nextest, which runs one
# process per test (ten nested `cargo build`s competing for the same lock).
#
# --no-default-features: neither fixture is gated on any feature, so building
# them still exercises the exact binaries the tests spawn regardless of which
# features the test run itself uses. It also skips the `desktop` feature's
# tauri-build permission validation, which errors on this box independently
# of these fixtures.
set -eu
cargo build --no-default-features --bin tuic-acp-fixture-agent --bin tuic-mcp-fixture-server
