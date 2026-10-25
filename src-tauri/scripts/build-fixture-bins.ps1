$ErrorActionPreference = 'Stop'
# Match nextest's profile and, in CI, its feature graph instead of rebuilding dev.
$cargoArgs = @('build', '--profile', 'test', '--no-default-features')
if ($env:TUIC_FIXTURE_FEATURES) {
    $cargoArgs += @('--features', $env:TUIC_FIXTURE_FEATURES)
}
$cargoArgs += @('--bin', 'tuic-acp-fixture-agent', '--bin', 'tuic-mcp-fixture-server')
cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
