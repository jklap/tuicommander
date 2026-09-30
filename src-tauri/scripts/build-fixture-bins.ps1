$ErrorActionPreference = 'Stop'
cargo build --no-default-features --bin tuic-acp-fixture-agent --bin tuic-mcp-fixture-server
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
