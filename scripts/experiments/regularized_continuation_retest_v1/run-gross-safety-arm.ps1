param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$TestName,
    [Parameter(Mandatory = $true)][string]$OutcomePath,
    [Parameter(Mandatory = $true)][string]$StdoutPath,
    [Parameter(Mandatory = $true)][string]$StderrPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

try {
    foreach ($path in @($OutcomePath, $StdoutPath, $StderrPath, $CompletionPath)) {
        if (Test-Path -LiteralPath $path) {
            throw "refusing to overwrite arm output: $path"
        }
    }
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Executable $TestName --ignored --exact --nocapture --test-threads=1 1> $StdoutPath 2> $StderrPath
        $nativeExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previous
    }
    $outcomeCreated = Test-Path -LiteralPath $OutcomePath -PathType Leaf
    $completion = [ordered]@{
        schema = 'regularized-continuation-gross-safety-arm-completion/v1'
        success = ($nativeExitCode -eq 0 -and $outcomeCreated)
        wrapper_process_id = $PID
        native_exit_code = $nativeExitCode
        executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Executable).Hash.ToLowerInvariant()
        outcome_created = $outcomeCreated
        outcome_sha256 = if ($outcomeCreated) { (Get-FileHash -Algorithm SHA256 -LiteralPath $OutcomePath).Hash.ToLowerInvariant() } else { $null }
        stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $StdoutPath).Hash.ToLowerInvariant()
        stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $StderrPath).Hash.ToLowerInvariant()
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
    }
    $json = $completion | ConvertTo-Json -Depth 6
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
    $stream = [IO.File]::Open($CompletionPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    }
    finally {
        $stream.Dispose()
    }
    if (-not $completion.success) {
        exit 1
    }
    exit 0
}
catch {
    Write-Error $_
    exit 1
}
