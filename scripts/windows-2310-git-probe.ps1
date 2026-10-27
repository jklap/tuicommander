# Native standalone differential: no Rust test fixture or retained checkout is used.
$ErrorActionPreference = 'Stop'
$originalPath = $env:PATH
$captured = Join-Path $env:RUNNER_TEMP '2310-hook-path.json'
if (!(Test-Path $captured)) { Write-Host 'No failed-hook PATH captured'; exit 0 }
$hookPath = Get-Content -Raw $captured | ConvertFrom-Json
$root = Join-Path $env:RUNNER_TEMP '2310-standalone-git'
New-Item -ItemType Directory -Path $root | Out-Null
$launcher = 'C:\Program Files\Git\cmd\git.exe'
& $launcher init --bare (Join-Path $root 'origin.git')
if ($LASTEXITCODE) { throw 'Bare repository setup failed' }
& $launcher init (Join-Path $root 'checkout')
if ($LASTEXITCODE) { throw 'Checkout setup failed' }
& $launcher -C (Join-Path $root 'checkout') remote add origin (Join-Path $root 'origin.git')
if ($LASTEXITCODE) { throw 'Origin setup failed' }
foreach ($mode in @('inherited', 'hook')) {
    foreach ($git in @($launcher, 'C:\Program Files\Git\bin\git.exe', 'C:\Program Files\Git\ucrt64\bin\git.exe')) {
        if (!(Test-Path $git)) { continue }
        foreach ($arguments in @(@('--version'), @('fetch', '--prune', 'origin'))) {
            $info = [Diagnostics.ProcessStartInfo]::new($git)
            $info.WorkingDirectory = Join-Path $root 'checkout'
            $info.UseShellExecute = $false
            $info.RedirectStandardOutput = $true
            $info.RedirectStandardError = $true
            $info.Environment['PATH'] = if ($mode -eq 'hook') { $hookPath } else { $originalPath }
            foreach ($argument in $arguments) { $info.ArgumentList.Add($argument) }
            $process = [Diagnostics.Process]::Start($info)
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            if (!$process.WaitForExit(10000)) {
                $process.Kill($true)
                throw "Standalone probe timed out: $mode $git $arguments"
            }
            Write-Host "STANDALONE mode=$mode git=$git args=$arguments exit=$($process.ExitCode) stdout=$($stdout.Result) stderr=$($stderr.Result)"
            $process.Dispose()
        }
    }
}
