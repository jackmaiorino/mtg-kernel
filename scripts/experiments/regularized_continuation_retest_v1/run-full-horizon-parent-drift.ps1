param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$TestName,
    [Parameter(Mandatory = $true)][string]$RequestPath,
    [Parameter(Mandatory = $true)][string]$ReportPath,
    [Parameter(Mandatory = $true)][string]$StdoutPath,
    [Parameter(Mandatory = $true)][string]$StderrPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

try {
    foreach ($path in @($ReportPath, $StdoutPath, $StderrPath, $CompletionPath)) {
        if (Test-Path -LiteralPath $path) {
            throw "refusing to overwrite parent-drift output: $path"
        }
    }
    $env:REGCONT_FULL_HORIZON_DRIFT_INPUT_JSON = $RequestPath
    $env:REGCONT_FULL_HORIZON_DRIFT_OUTPUT_JSON = $ReportPath
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Executable $TestName --ignored --exact --nocapture --test-threads=1 1> $StdoutPath 2> $StderrPath
        $nativeExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previous
        Remove-Item -Path Env:REGCONT_FULL_HORIZON_DRIFT_INPUT_JSON -ErrorAction SilentlyContinue
        Remove-Item -Path Env:REGCONT_FULL_HORIZON_DRIFT_OUTPUT_JSON -ErrorAction SilentlyContinue
    }
    $reportCreated = Test-Path -LiteralPath $ReportPath -PathType Leaf
    $completion = [ordered]@{
        schema = 'regularized-continuation-full-horizon-parent-drift-completion/v1'
        success = ($nativeExitCode -eq 0 -and $reportCreated)
        wrapper_process_id = $PID
        native_exit_code = $nativeExitCode
        executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Executable).Hash.ToLowerInvariant()
        request_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $RequestPath).Hash.ToLowerInvariant()
        report_created = $reportCreated
        report_sha256 = if ($reportCreated) { (Get-FileHash -Algorithm SHA256 -LiteralPath $ReportPath).Hash.ToLowerInvariant() } else { $null }
        stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $StdoutPath).Hash.ToLowerInvariant()
        stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $StderrPath).Hash.ToLowerInvariant()
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
    }
    $temporary = "$CompletionPath.tmp-$PID"
    $completion | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $temporary -Encoding utf8
    Move-Item -LiteralPath $temporary -Destination $CompletionPath
    if (-not $completion.success) {
        exit 1
    }
    exit 0
}
catch {
    Write-Error $_
    exit 1
}
