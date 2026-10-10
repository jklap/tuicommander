# Nextest setup script (Windows): same roots as nextest-test-tmp.sh.
$ErrorActionPreference = 'Stop'
$hostTmp = if ($env:TUIC_TEST_HOST_TMPDIR) { $env:TUIC_TEST_HOST_TMPDIR } else { [System.IO.Path]::GetTempPath() }
$hostTmp = $hostTmp.TrimEnd('\', '/')
if ($env:TUIC_TEST_TMP_ROOT) {
    $root = $env:TUIC_TEST_TMP_ROOT
} else {
    $base = if ($env:TUIC_TEST_TMP_BASE) { $env:TUIC_TEST_TMP_BASE } else { Join-Path $hostTmp 'tuic-tests' }
    $checkout = (Resolve-Path '..').Path
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $digest = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($checkout))
    $hash = -join ($digest[0..7] | ForEach-Object { $_.ToString('x2') })
    $root = Join-Path $base "tuic-nextest-$hash"
}
New-Item -ItemType Directory -Path $root -Force | Out-Null
$root = (Resolve-Path $root).Path
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
