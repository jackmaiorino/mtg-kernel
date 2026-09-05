# run_cp7_cycle_mid_4shard_v1.ps1
#
# Sibling of run_cp7_cycle_start_4shard_v1.ps1 / run_cp7_cycle_end_4shard_v1.ps1
# for the cycle-3 MID-CYCLE CP7 read (the second of the three registered
# reads: cycle-start, mid-cycle, cycle-end -- see
# CLAUDE-CYCLE3-READS-PHASE-NOTES-V1.md Section 7). Same structure, same
# acceptance/failure/void-cap machinery, unchanged from the other two
# wrappers except:
#   - base_seed 2026082802 (the mid-cycle seed), distinct from cycle-start's
#     2026082801 and cycle-end's 2026082803.
#   - focal is the store's GENERATION-512 checkpoint (population:512:...),
#     the midpoint of the trajectory this program's own 16-refresh-boundary
#     cycle spans (start=0, mid=512, end=2048 in this store's own indexing).
#   - evidence-root naming: mid-cycle-g0512-attempt-001.
#
# void-cap-mode: report, unchanged rationale -- this script is scoped to the
# registered reads, and Jack's ruling (relayed by the coordinator; no
# independently located written record in collab at implementation time)
# waives cap enforcement for all three under full disclosure.
# Accounting/reporting stay mandatory either way.
#
# See run_cp7_cycle_end_4shard_v1.ps1's own header comments for the full
# Template A provenance, the PS 5.1 native-stderr-as-terminating-error fix,
# and the Amendment 4 A4.2 read-level accumulation design -- all identical
# here, not re-derived.

$ErrorActionPreference = "Stop"

$pythonExe = "C:\Users\Jack\AppData\Local\Microsoft\WindowsApps\python3.exe"
$scriptDir = "C:\Users\Jack\IdeaProjects\mtg-kernel-cycle3-campaign-fable\scripts\current_net8_cp7_population_store_panel_v2"
$parentEvidenceRoot = "E:\mtg-kernel-population-v2-cycle3-cp7-anchor-reads\mid-cycle-g0512-attempt-001"
$baseSeed = "2026082802"
$scorerExe = "D:\cargo-target-throughput-remeasure-v1\release\checkpoint_shadow_stdio_v1.exe"
$mageRepo = "C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1-a1d4be43-pin"
$sourceDatabase = "E:\mtg-kernel-population-v2-cycle3-cp7-anchor-reads\carddb-staging\cards.h2.mv.db"
$maven = "C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd"
$readPairs = 512
$readVoidCap = 10
$voidCapMode = "report"

$focalModel = "focal=population:512:E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003\run-0\store"
$referenceModel = "reference=population:2048:E:\mtg-kernel-population-v2-cycle3\parent-import\current-1-seed-975002-store\run-0\store"
$anchorModel = "anchor=original:384:D:\mtg-kernel-ladder-pilot-20260725\pool3\primary"

New-Item -ItemType Directory -Path $parentEvidenceRoot -Force | Out-Null
$logPath = Join-Path $parentEvidenceRoot "_wrapper-progress.log"
Push-Location $scriptDir

"wrapper start $(Get-Date -Format o)" | Out-File -FilePath $logPath -Encoding utf8 -Append

$shardOffsets = @(0, 128, 256, 384)
$voidTotals = [ordered]@{ focal = 0; reference = 0; anchor = 0 }
$everBreachedModels = @()

for ($i = 0; $i -lt $shardOffsets.Length; $i++) {
    $offset = $shardOffsets[$i]
    $shardName = "shard-{0:D3}" -f $i
    $shardRoot = Join-Path $parentEvidenceRoot $shardName
    $shardStdout = Join-Path $parentEvidenceRoot "$shardName-stdout.log"
    $shardStderr = Join-Path $parentEvidenceRoot "$shardName-stderr.log"

    "launching $shardName pair-start=$offset at $(Get-Date -Format o)" | Out-File -FilePath $logPath -Encoding utf8 -Append

    $exitCode = $null
    $caughtException = $null
    try {
        $cmdLine = "`"$pythonExe`" `"run_cp7_store_panel_v2.py`"" `
            + " --evidence-root `"$shardRoot`"" `
            + " --model `"$focalModel`"" `
            + " --model `"$referenceModel`"" `
            + " --model `"$anchorModel`"" `
            + " --mode formal" `
            + " --base-seed $baseSeed" `
            + " --pair-start $offset" `
            + " --pairs 128" `
            + " --read-pairs $readPairs" `
            + " --void-cap-mode $voidCapMode" `
            + " --workers 8" `
            + " --task-pairs 32" `
            + " --scorer-exe `"$scorerExe`"" `
            + " --mage-repo `"$mageRepo`"" `
            + " --source-database `"$sourceDatabase`"" `
            + " --maven `"$maven`"" `
            + " --tolerate-engine-faults" `
            + " > `"$shardStdout`" 2> `"$shardStderr`""
        cmd.exe /c $cmdLine
        $exitCode = $LASTEXITCODE
    } catch {
        $caughtException = $_.Exception.Message
        $exitCode = -1
        "shard $shardName raised a PowerShell exception (not a normal nonzero exit): $caughtException" | Out-File -FilePath $logPath -Encoding utf8 -Append
    }

    $summaryExists = Test-Path (Join-Path $shardRoot "panel-summary.json")

    "shard $shardName exit=$exitCode summary_present=$summaryExists at $(Get-Date -Format o)" | Out-File -FilePath $logPath -Encoding utf8 -Append

    if ($exitCode -ne 0 -or -not $summaryExists) {
        $failRecord = [ordered]@{
            status = "FAILED"
            stage = "shard_failure"
            failed_shard = $shardName
            pair_start = $offset
            exit_code = $exitCode
            summary_present = $summaryExists
            powershell_exception = $caughtException
            completed_at = (Get-Date -Format o)
        }
        try {
            $failRecord | ConvertTo-Json | Out-File -FilePath (Join-Path $parentEvidenceRoot "PANEL_FAILED.json") -Encoding utf8
        } catch {
            "FATAL: could not write PANEL_FAILED.json: $($_.Exception.Message)" | Out-File -FilePath $logPath -Encoding utf8 -Append
        }
        "STOP: $shardName failed (stage=shard_failure); not proceeding to remaining shards." | Out-File -FilePath $logPath -Encoding utf8 -Append
        Pop-Location
        exit 1
    }

    $shardSummary = Get-Content (Join-Path $shardRoot "panel-summary.json") -Raw | ConvertFrom-Json
    foreach ($label in @($voidTotals.Keys)) {
        $perModel = $shardSummary.voids.per_model.$label
        if ($null -eq $perModel) {
            "FATAL: $shardName panel-summary.json has no voids.per_model entry for '$label'." | Out-File -FilePath $logPath -Encoding utf8 -Append
            Pop-Location
            exit 1
        }
        $voidTotals[$label] += [int]$perModel.voided_pairs
    }
    "shard $shardName void totals so far: $((@($voidTotals.Keys) | ForEach-Object { "$_=$($voidTotals[$_])" }) -join ', ')" | Out-File -FilePath $logPath -Encoding utf8 -Append

    $breachingModels = @($voidTotals.Keys) | Where-Object { ($voidTotals[$_] * 100) -gt ($readPairs * 2) }
    if ($breachingModels.Count -gt 0) {
        foreach ($label in $breachingModels) {
            if ($everBreachedModels -notcontains $label) { $everBreachedModels += $label }
        }
        if ($voidCapMode -eq "enforce") {
            $readFailRecord = [ordered]@{
                status = "FAILED"
                stage = "read_level_void_cap"
                failed_after_shard = $shardName
                read_pairs = $readPairs
                read_void_cap = $readVoidCap
                void_totals = $voidTotals
                breaching_models = $breachingModels
                completed_at = (Get-Date -Format o)
            }
            try {
                $readFailRecord | ConvertTo-Json | Out-File -FilePath (Join-Path $parentEvidenceRoot "PANEL_FAILED.json") -Encoding utf8
            } catch {
                "FATAL: could not write PANEL_FAILED.json: $($_.Exception.Message)" | Out-File -FilePath $logPath -Encoding utf8 -Append
            }
            "STOP: read-level void cap breached after $shardName (stage=read_level_void_cap): $($breachingModels -join ', '); not proceeding to remaining shards." | Out-File -FilePath $logPath -Encoding utf8 -Append
            Pop-Location
            exit 1
        }
        "NOTE (void-cap-mode=report, not enforced): read-level void cap breached after ${shardName}: $($breachingModels -join ', ') -- continuing." | Out-File -FilePath $logPath -Encoding utf8 -Append
    }
}

$doneRecord = [ordered]@{
    status = "ALL_SHARDS_COMPLETE"
    shard_count = $shardOffsets.Length
    base_seed = $baseSeed
    pair_start_offsets = $shardOffsets
    pairs_per_shard = 128
    total_pairs = 512
    read_pairs = $readPairs
    read_void_cap = $readVoidCap
    void_totals = $voidTotals
    void_cap_mode = $voidCapMode
    read_level_cap_breached_models = $everBreachedModels
    completed_at = (Get-Date -Format o)
}
$doneRecord | ConvertTo-Json | Out-File -FilePath (Join-Path $parentEvidenceRoot "PANEL_DONE.json") -Encoding utf8

"wrapper done, PANEL_DONE.json written at $(Get-Date -Format o)" | Out-File -FilePath $logPath -Encoding utf8 -Append
Pop-Location
