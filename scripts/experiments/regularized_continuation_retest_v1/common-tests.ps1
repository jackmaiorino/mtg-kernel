$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("regularized-continuation-common-test-$([guid]::NewGuid().ToString('N'))")
New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
    $executable = Join-Path $testRoot 'fake-executable.bin'
    $log = Join-Path $testRoot 'lane.log'
    $stderr = Join-Path $testRoot 'lane.stderr.log'
    $completionPath = Join-Path $testRoot 'lane.completion.json'
    'fake executable bytes' | Set-Content -LiteralPath $executable -Encoding utf8
    "MULTIRUN AGGREGATE runs=1 episodes=512`ntest result: ok." | Set-Content -LiteralPath $log -Encoding utf8
    [IO.File]::WriteAllBytes($stderr, [byte[]]@())
    $processId = 4242
    $lane = [pscustomobject]@{
        gpu_ordinal = 1
        store_parent = 'D:\synthetic-store'
        log = $log
        stderr = $stderr
        completion = $completionPath
        executable = $executable
        seed = [uint64]969999
        updates = [uint64]8
        policy_anchor_beta = '0'
    }
    $completion = [ordered]@{
        schema = 'regularized-continuation-native-lane-completion/v1'
        success = $true
        process_id = $processId
        executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
        seed = $lane.seed
        updates = $lane.updates
        store_parent = $lane.store_parent
        gpu_ordinal = $lane.gpu_ordinal
        policy_anchor_beta = $lane.policy_anchor_beta
        log_path = $lane.log
        log_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $log).Hash.ToLowerInvariant()
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
    }
    Write-JsonFile -Value $completion -Path $completionPath
    Get-VerifiedNativeLaneCompletion -Lane $lane -ProcessId $processId | Out-Null

    Add-Content -LiteralPath $log -Value 'tamper'
    $blocked = $false
    try {
        Get-VerifiedNativeLaneCompletion -Lane $lane -ProcessId $processId | Out-Null
    }
    catch {
        $blocked = $_.Exception.Message -match 'hashes do not match'
    }
    if (-not $blocked) {
        throw 'tampered child log was not rejected'
    }
    Write-Host 'COMMON HARNESS TESTS PASS'
}
finally {
    Remove-Item -LiteralPath $testRoot -Recurse -Force
}
