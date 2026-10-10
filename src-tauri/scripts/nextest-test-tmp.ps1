$ErrorActionPreference = 'Stop'
$root = if ($env:TUIC_TEST_TMP_ROOT) { $env:TUIC_TEST_TMP_ROOT } else { Join-Path (Resolve-Path '..') '.tmp/tuic-tests' }
New-Item -ItemType Directory -Path $root -Force | Out-Null
$root = (Resolve-Path $root).Path
# Empty template dir: test-fixture `git init`/`git clone` skip git's hooks/*.sample
# copies (see nextest-test-tmp.sh).
$gitTemplate = Join-Path $root 'git-template-empty'
New-Item -ItemType Directory -Path $gitTemplate -Force | Out-Null
# Fail closed: git never discovers a repository above the root (see
# nextest-test-tmp.sh). Append the root's parent once to any inherited value.
$gitCeiling = if ($env:GIT_CEILING_DIRECTORIES) { $env:GIT_CEILING_DIRECTORIES } else { '' }
$ceilingParent = Split-Path -Parent $root
if (-not (($gitCeiling -split ';') -contains $ceilingParent)) {
    $gitCeiling = if ($gitCeiling) { "$gitCeiling;$ceilingParent" } else { $ceilingParent }
}
$lines = @(
    "TUIC_TEST_TMP_ROOT=$root"
    "TMPDIR=$root"
    "TMP=$root"
    "TEMP=$root"
    "GIT_TEMPLATE_DIR=$gitTemplate"
    "GIT_CEILING_DIRECTORIES=$gitCeiling"
)
[System.IO.File]::AppendAllText(
    $env:NEXTEST_ENV,
    ($lines -join "`n") + "`n",
    [System.Text.UTF8Encoding]::new($false)
)
