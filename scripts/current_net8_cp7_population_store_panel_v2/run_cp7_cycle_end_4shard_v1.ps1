# run_cp7_cycle_end_4shard_v1.ps1
#
# Runs the cycle-3 cycle-end CP7 read as 4 sequential formal shards, mirroring the
# family's own Template A convention exactly (D:\mtg-kernel-cp7-anchor-panel-v3\
# gen1024-lineages\shard-000..003, verified from those shards' own panel-plan.json
# "panel" blocks on 2026-08-28):
#   - base_seed is IDENTICAL across all 4 shards (no per-shard seed derivation) --
#     shard identity is carried entirely by --pair-start.
#   - --pair-start advances 0 / 128 / 256 / 384 across shard-000..003.
#   - --pairs is 128 in every shard (run_cp7_store_panel_v2.py hard-requires
#     args.pairs == 128 for --mode formal; this is not configurable).
#   - --task-pairs 32, --workers 8 (matches Template A's panel.task_pairs /
#     panel.workers fields).
#   - --tolerate-engine-faults (matches Template A's panel.tolerate_engine_faults
#     = true in every shard, and CP7-ANCHOR-PANEL-V3-RESULT.md's cell-3 command).
#   - evidence-root naming: shard-000..003 under one parent evidence root
#     (Template A: gen1024-lineages\; here: cycle-end-g2048\).
#
# Aggregation in Template A was a separate post-hoc step (re-validating each
# shard's raw outcome.jsonl through validate_outcome_shard and summing across the
# 4 panel-summary.json files into panel-v3-analysis.json) -- NOT something the
# launcher itself does. This wrapper only drives the 4 shard invocations and
# records completion; cross-shard aggregation is a later, separate analysis step.
#
# Runs sequentially; per the family's own precedent of not proceeding past a
# failed shard (CP7-ANCHOR-PANEL-V3-RESULT.md, cell 3 attempt 2: "Cell 3 did not
# proceed past shard-000"), this wrapper stops immediately (writes
# PANEL_FAILED.json, does not launch remaining shards) if any shard exits
# nonzero or does not produce panel-summary.json.
#
# On all 4 shards completing cleanly, writes PANEL_DONE.json under the parent
# evidence root (family's own equivalent marker name: ALL-DONE.json /
# cell3-ALL-DONE.json).
#
# This script is intended to be launched itself via a detached Start-Process
# (see the launching PowerShell session's own recipe) -- it blocks internally
# (each shard runs to completion, checked via $LASTEXITCODE, before the next is
# launched) but the OUTER invocation of this script must be detached so the
# calling session is not blocked for the read's full duration.
#
# AMENDMENT 4 A4.2 (this revision, 2026-08-28): void-cap accounting moved
# from per-shard (2% of 128 pairs, historical) to per-read (2% of the full
# 512-pair read = 10 voided pairs per model, Jack's ruling on
# CLAUDE-QUALIFIED-SCORER-DECODE-WIDENING-PORT-PLAN-V1.md Amendment 4).
# `run_cp7_store_panel_v2.py` itself only ever sees one shard at a time, so
# it now enforces only a per-shard SANITY CEILING (--read-pairs 512, fails
# fast if a single shard alone already exceeds the read-level cap; this
# reuses the existing per-shard PANEL_FAILED.json path below unchanged,
# since that IS a genuine, correctly-attributed shard failure). The real
# per-read cap is enforced HERE: after every shard, this wrapper reads that
# shard's own panel-summary.json void counts, accumulates a running
# per-model total across all shards completed so far, and stops with a
# distinctly-shaped marker (PANEL_FAILED.json, stage="read_level_void_cap")
# if any model's accumulated total crosses $readVoidCap -- never confused
# with a single shard's own crash/exit-code failure (stage="shard_failure"
# below). $readVoidCap must match run_cp7_store_panel_v2.py's own
# VOID_CAP_FRACTION_NUMERATOR/_DENOMINATOR arithmetic at $readPairs exactly
# (2 * 512 / 100 = 10.24, floor 10); this is intentionally duplicated
# across Python and PowerShell and must be kept in sync by hand.
#
# FIX (earlier revision, post-incident): the first real launch of this script
# (shard-000, 2026-08-28 08:38-08:47) crashed the whole wrapper WITHOUT
# writing PANEL_FAILED.json, because `& $pythonExe ... > $stdout 2> $stderr`
# is a native-command invocation whose stderr PowerShell 5.1 maps onto the
# error stream regardless of the `2>` file redirect; with
# $ErrorActionPreference="Stop", any non-empty stderr line from the native
# process (here, a real engine fault the python panel runner reported on its
# own stderr) became a terminating NativeCommandError that unwound the
# script before the exit-code-check/PANEL_FAILED.json logic below ever ran.
# Fixed by invoking through `cmd.exe /c` with an explicit quoted command
# line: cmd.exe performs the `>`/`2>` redirection itself at the OS level, so
# PowerShell never sees the native process's stderr as its own error stream
# at all, and $LASTEXITCODE is read reliably afterward (the same fix this
# session already adopted for exit-code retrieval elsewhere, for the same
# underlying PS 5.1 behavior). The whole per-shard body is additionally
# wrapped in try/catch as defense in depth, so ANY unexpected PowerShell-level
# exception (not just this specific one) still results in a PANEL_FAILED.json
# write before the script exits, never a silent, marker-less death.

$ErrorActionPreference = "Stop"

$pythonExe = "C:\Users\Jack\AppData\Local\Microsoft\WindowsApps\python3.exe"
$scriptDir = "C:\Users\Jack\IdeaProjects\mtg-kernel-cycle3-campaign-fable\scripts\current_net8_cp7_population_store_panel_v2"
$parentEvidenceRoot = "E:\mtg-kernel-population-v2-cycle3-cp7-anchor-reads\cycle-end-g2048-attempt-005"
$baseSeed = "2026082803"
$scorerExe = "D:\cargo-target-throughput-remeasure-v1\release\checkpoint_shadow_stdio_v1.exe"
$mageRepo = "C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1-a1d4be43-pin"
$sourceDatabase = "E:\mtg-kernel-population-v2-cycle3-cp7-anchor-reads\carddb-staging\cards.h2.mv.db"
$maven = "C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd"
$readPairs = 512
$readVoidCap = 10
# Jack's ruling (relayed by the coordinator, this session; no
# independently located written record in collab at implementation time --
# cited as relayed, not verified against a source document): "the void cap
# is waived for the three registered CP7 reads under full disclosure;
# accounting and reporting stay mandatory, enforcement becomes report-only
# for these reads." This wrapper is scoped exactly to those registered
# reads, so it always passes "report": accounting (per-shard sanity
# ceiling data, per-model void counts, the read-level running total) is
# computed and recorded identically either way; only whether a breach
# stops the read differs. A fresh future wrapper for non-registered reads
# should default this to "enforce".
$voidCapMode = "report"

$focalModel = "focal=population:2048:E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003\run-0\store"
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
        # cmd.exe performs the redirection itself; PowerShell never sees the
        # native process's stderr as its own error stream, so a real engine
        # fault reported on stderr cannot become a terminating
        # NativeCommandError here (see FIX note above).
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

    # AMENDMENT 4 A4.2: accumulate this shard's own per-model void counts
    # (panel-summary.json's existing voids.per_model shape, unchanged by
    # this amendment) into the running read-level total, then enforce the
    # real per-read cap here -- run_cp7_store_panel_v2.py's own per-shard
    # sanity ceiling above cannot see prior shards, so this accumulation is
    # the only place the read-level cap is actually enforced.
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

    # readVoidCap*100 > readPairs*2 is the exact VOID_CAP_FRACTION_NUMERATOR/
    # _DENOMINATOR arithmetic run_cp7_store_panel_v2.py's own
    # void_cap_breaches() implements; kept identical here by hand (see the
    # amendment note at the top of this file).
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
        # void-cap-mode=report: Jack's ruling waives enforcement for the
        # registered reads. Recorded (this log line, plus $everBreachedModels
        # carried into PANEL_DONE.json below), never stops the read.
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
    # AMENDMENT 4 A4.2: the final per-model accumulated void tally over the
    # full read, read back from the completion marker directly rather than
    # re-derived by hand from all 4 shards' own panel-summary.json files.
    read_pairs = $readPairs
    read_void_cap = $readVoidCap
    void_totals = $voidTotals
    void_cap_mode = $voidCapMode
    # Every model that ever crossed the read-level cap during accumulation,
    # even though void-cap-mode=report never stopped the read for it --
    # Jack's ruling requires accounting/reporting to stay mandatory.
    read_level_cap_breached_models = $everBreachedModels
    completed_at = (Get-Date -Format o)
}
$doneRecord | ConvertTo-Json | Out-File -FilePath (Join-Path $parentEvidenceRoot "PANEL_DONE.json") -Encoding utf8

"wrapper done, PANEL_DONE.json written at $(Get-Date -Format o)" | Out-File -FilePath $logPath -Encoding utf8 -Append
Pop-Location
