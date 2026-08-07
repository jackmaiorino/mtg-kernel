param(
    # 'smoke' runs exactly one arm (promoted2-slot0) at -SmokePairCount pairs
    # to prove plumbing. 'full' runs all three fixed arms at 512 pairs each.
    # Fixed arm list either way; no selection after reads.
    [ValidateSet('smoke', 'full')][string]$RunMode = 'smoke',
    [uint64]$SmokePairCount = 4,
    [string]$EvidenceRoot = 'D:\mtg-kernel-mirror-seat-diagnostic-v1'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

# Fixed arm list (declared before any execution; identities and exact SHAs
# are the ones verified and cited in CLAUDE-MIRROR-SEAT-DIAGNOSTIC-V1.md
# Section 2 against D:\mtg-kernel-scaled-selfplay-population-v1\active\
# refresh-008\attempt-001\refresh-008.json (manifest SHA-256
# 9c9490b205b7b5a933eae7ca86916e5ff5ff9307a150dc35487a8e1c28e73e22) and the
# response-exploiter-checkpoint-diagnostic attempt-002 evidence that first
# established slot 0 == promoted(2) byte-for-byte.
#
# mirror-promoted2-slot0: population slot 0 is EXACTLY promoted(2) generation
#   384 (identical run_sha256, checkpoint_manifest_sha256, checkpoint_payload_
#   sha256) -- confirmed by direct SHA-256 comparison during design, see the
#   sheet Section 0. One arm answers both the "promoted(2)" question and the
#   "slot 0" question; it is not run twice.
$arms = @(
    [ordered]@{
        label = 'mirror-promoted2-slot0'
        store_root = 'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary'
        generation = [uint64]384
        evaluation_seed = [uint64]1975001
        expected_checkpoint_manifest_sha256 = '4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8'
        expected_checkpoint_payload_sha256 = 'a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99'
        decomposition_reference_slot = 0
    },
    [ordered]@{
        label = 'mirror-slot3'
        store_root = 'D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\wave-00-seed-970002-gpu1\run-0\store'
        generation = [uint64]1152
        evaluation_seed = [uint64]1975002
        expected_checkpoint_manifest_sha256 = '7cf89f863e9a84b3a5769cd2e60f86f5a0896ae984f741dc5ca8430050c8eee0'
        expected_checkpoint_payload_sha256 = 'd020d5ad617d004d2590b50a3b164fd6ae789662e185f57829d6c1bec34e62e0'
        decomposition_reference_slot = 3
    },
    [ordered]@{
        label = 'mirror-slot5'
        store_root = 'D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\wave-00-seed-970001-gpu0\run-0\store'
        generation = [uint64]1536
        evaluation_seed = [uint64]1975003
        expected_checkpoint_manifest_sha256 = '8969200dc54c1fef1184a984de7ae766ab94ce87cc73e51df17e8c22989f8af1'
        expected_checkpoint_payload_sha256 = '8dfc0659d950607b9fa64be886b48614a529fff079e13f22514860fe5e0973a5'
        decomposition_reference_slot = 5
    }
)

function Assert-ArmIdentityV1 {
    param([Parameter(Mandatory = $true)]$Arm)
    $root = (Resolve-Path -LiteralPath $Arm.store_root).Path
    $prefix = Join-Path $root ('checkpoints\update-{0:d8}' -f [uint64]$Arm.generation)
    foreach ($suffix in @('checkpoint.json', 'sidecar.json', 'state.f32le')) {
        if (-not (Test-Path -LiteralPath "$prefix.$suffix" -PathType Leaf)) { throw "arm $($Arm.label): checkpoint file missing: $prefix.$suffix" }
    }
    $checkpointSha = Get-Sha256V1 "$prefix.checkpoint.json"
    $stateSha = Get-Sha256V1 "$prefix.state.f32le"
    if ($checkpointSha -cne $Arm.expected_checkpoint_manifest_sha256) { throw "arm $($Arm.label): checkpoint manifest SHA differs from the population-evidence-bound value" }
    if ($stateSha -cne $Arm.expected_checkpoint_payload_sha256) { throw "arm $($Arm.label): checkpoint payload SHA differs from the population-evidence-bound value" }
}

try {
    New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
    $attemptRoot = Join-Path $EvidenceRoot ("{0}-{1:yyyyMMdd-HHmmss}" -f $RunMode, (Get-Date).ToUniversalTime())
    New-Item -ItemType Directory -Path $attemptRoot | Out-Null

    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $executable = Get-ReleaseTestExecutableV1 -RepoRoot $script:RepoRoot -EvidenceRoot $attemptRoot -Label 'mirror-seat-diagnostic'
    Write-Host "MIRROR PANEL executable=$executable git_commit=$($git.commit) dirty=$($git.dirty)"

    foreach ($arm in $arms) { Assert-ArmIdentityV1 -Arm $arm }
    Write-Host 'MIRROR PANEL all fixed-arm checkpoint identities verified against population-evidence-bound SHA-256 values'

    $selected = if ($RunMode -eq 'smoke') { @($arms[0]) } else { $arms }
    $pairCount = if ($RunMode -eq 'smoke') { $SmokePairCount } else { [uint64]512 }

    $runs = @()
    foreach ($arm in $selected) {
        $outcome = Join-Path $attemptRoot "$($arm.label).outcome.json"
        $stdout = Join-Path $attemptRoot "$($arm.label).stdout.log"
        $stderr = Join-Path $attemptRoot "$($arm.label).stderr.log"
        $completion = Join-Path $attemptRoot "$($arm.label).completion.json"
        $args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'run-mirror-arm.ps1'),
            '-Label', $arm.label, '-Executable', $executable, '-StoreRoot', $arm.store_root, '-Generation', [string]$arm.generation,
            '-PairCount', [string]$pairCount, '-EvaluationSeed', [string]$arm.evaluation_seed,
            '-OutcomePath', $outcome, '-StdoutPath', $stdout, '-StderrPath', $stderr, '-CompletionPath', $completion)
        $wrapperOut = Join-Path $attemptRoot "$($arm.label).wrapper.stdout.log"
        $wrapperErr = Join-Path $attemptRoot "$($arm.label).wrapper.stderr.log"
        $process = Start-Process -FilePath 'powershell.exe' -ArgumentList $args -WorkingDirectory $script:RepoRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $wrapperOut -RedirectStandardError $wrapperErr
        $runs += [ordered]@{ arm = $arm; process = $process; outcome = $outcome; stdout = $stdout; stderr = $stderr; completion = $completion; wrapper_stdout = $wrapperOut; wrapper_stderr = $wrapperErr }
    }
    while (@($runs | Where-Object { -not $_.process.HasExited }).Count -gt 0) { Start-Sleep -Seconds 2; foreach ($run in $runs) { $run.process.Refresh() } }

    # Outcome-blind gate: every arm's completion is validated for SUCCESS
    # before any outcome JSON is read/parsed by anyone (the analyzer is
    # invoked only after this loop, and only reads terminal W/L/D ranks).
    $records = @()
    foreach ($run in $runs) {
        $run.process.WaitForExit()
        if (-not (Test-Path -LiteralPath $run.completion)) { throw "mirror arm produced no completion record: $($run.arm.label)" }
        $completion = Get-Content -Raw -LiteralPath $run.completion | ConvertFrom-Json
        if ($completion.schema -ne 'mtg-kernel-mirror-seat-diagnostic-arm-completion/v1' -or $completion.success -ne $true -or [int]$completion.native_exit_code -ne 0 -or -not $completion.outcome_created) {
            throw "mirror arm did not complete successfully: $($run.arm.label); see $($run.stderr)"
        }
        $records += [ordered]@{ label = $run.arm.label; decomposition_reference_slot = $run.arm.decomposition_reference_slot; completion = Get-FileRecordV1 $run.completion; outcome = Get-FileRecordV1 $run.outcome; stdout = Get-FileRecordV1 $run.stdout; stderr = Get-FileRecordV1 $run.stderr }
    }

    $manifest = [ordered]@{
        schema = 'mtg-kernel-mirror-seat-diagnostic-execution/v1'
        run_mode = $RunMode; pair_count = $pairCount
        git = $git; executable = [ordered]@{ path = $executable; sha256 = Get-Sha256V1 $executable }
        arms = $records
        all_arms_completed_before_any_read = $true
        no_selection_after_reads = $true
    }
    $manifestPath = Join-Path $attemptRoot 'panel-manifest.json'
    Write-NewJsonV1 $manifest $manifestPath
    Write-Host "MIRROR PANEL complete run_mode=$RunMode evidence=$manifestPath"
} catch { throw }
