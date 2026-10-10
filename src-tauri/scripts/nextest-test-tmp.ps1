# Nextest setup script (Windows): same roots as nextest-test-tmp.sh, in the
# order scripts/test-tmp-lib.sh documents.
$ErrorActionPreference = 'Stop'
$hostTmp = if ($env:TUIC_TEST_HOST_TMPDIR) { $env:TUIC_TEST_HOST_TMPDIR } else { [System.IO.Path]::GetTempPath() }
$hostTmp = $hostTmp.TrimEnd('\', '/')
if ($env:TUIC_TEST_TMP_ROOT) {
    $root = $env:TUIC_TEST_TMP_ROOT
} else {
    $base = if ($env:TUIC_TEST_TMP_BASE) { $env:TUIC_TEST_TMP_BASE.TrimEnd('\', '/') } else { Join-Path $hostTmp 'tuic-tests' }
    # FNV-1a 64 of the checkout path, as tuic-test-support's checkout_hash().
    $checkout = (Resolve-Path '..').ProviderPath
    $mod = [System.Numerics.BigInteger]::Pow(2, 64)
    $hash = [System.Numerics.BigInteger]::Parse('14695981039346656037')
    foreach ($byte in [System.Text.Encoding]::UTF8.GetBytes($checkout)) {
        $low = [int]($hash % 256)
        $hash = (($hash - $low + ($low -bxor $byte)) * 1099511628211) % $mod
    }
    $root = Join-Path $base ('tuic-co-' + ([uint64]$hash).ToString('x16'))
}
New-Item -ItemType Directory -Path $root -Force | Out-Null
$root = (Resolve-Path $root).ProviderPath
$lines = @(
    "TUIC_TEST_HOST_TMPDIR=$hostTmp"
    "TUIC_TEST_TMP_ROOT=$root"
    "TMPDIR=$root"
    "TMP=$root"
    "TEMP=$root"
)
[System.IO.File]::AppendAllText(
    $env:NEXTEST_ENV,
    ($lines -join "`n") + "`n",
    [System.Text.UTF8Encoding]::new($false)
)
