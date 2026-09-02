Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

<#
Test-time-search wrapper, stage S1: the pinned report contract.

Every string here is a COMPILED CONSTANT on the Rust side
(native_tts_s1_replay_v1.rs). They are restated in this file for one purpose:
so the launcher can reject a tier report produced by a different rule BEFORE
it is summarized and before the run can be marked complete. They are never an
independent source of truth, and a disagreement is always resolved in the
Rust constant's favour by updating this file.

Why the whole contract and not just the top-level rule: the compute-cap block
NESTS the fitted latency curve, and the curve carries its own rule string.
A partially updated replay binary can therefore declare the current
compute-cap rule while the curve inside it was fitted under a superseded one,
which is precisely the shape of report that must not be allowed to mark a run
complete. Pinning only the outer rule would accept it.

This file lives apart from run-tts-s1.ps1 so the dry-run tests can dot-source
the same constants and the same validation the launcher uses, rather than
re-implementing either. It defines constants and functions only; it launches
nothing and has no side effects.
#>

# native_tts_s1_replay_v1::TTS_S1_S2_PROJECTION_RULE_V2
$script:TtsS1ProjectionRule = 'wrapped-games-only' +
    '-3072-root-clusters-times-2-paired-units' +
    '-isotonic-per-ordinal-protocol-latency-curve-fitted-to-whole-episode-timings' +
    '-extrapolated-past-the-last-observed-ordinal-at-the-maximum-adjacent-fitted-rise' +
    '-every-harvested-episode-natural-and-truncated-costed-at-its-own-length' +
    '-mean-estimated-episode-cost-times-wrapped-games' +
    '-as-aggregate-worker-hours-with-no-worker-division' +
    '/v2'

# native_tts_s1_replay_v1::TTS_S1_LATENCY_CURVE_RULE_V2
$script:TtsS1LatencyCurveRule = 'pool-adjacent-violators-isotonic-regression-over-decision-ordinal' +
    '-on-whole-episode-protocol-micros-pre-aggregated-per-ordinal' +
    '-extrapolated-past-the-last-observed-ordinal' +
    '-at-the-maximum-rise-between-adjacent-fitted-ordinals' +
    '-floored-at-one-micro-per-ordinal' +
    '/v2'

# native_tts_s1_replay_v1::TTS_S1_VERDICT_VIEW_V1
$script:TtsS1VerdictView = 'corpus_target_view'

function Get-TtsS1PinnedContract {
    # The whole pinned contract, for the provenance and summary records.
    return [ordered]@{
        compute_cap_rule = $script:TtsS1ProjectionRule
        latency_curve_rule = $script:TtsS1LatencyCurveRule
        verdict_view = $script:TtsS1VerdictView
    }
}

function Get-TtsS1ReportField {
    # Walks a dotted path through a decoded report and fails closed, by name,
    # on anything missing. Done through PSObject.Properties rather than plain
    # member access so a missing nested block reports WHICH field is absent
    # instead of a strict-mode PropertyNotFound on an inner segment.
    param(
        [Parameter(Mandatory = $true)][AllowNull()]$Report,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $current = $Report
    foreach ($segment in $Path.Split('.')) {
        if ($null -eq $current) {
            throw "tier report is missing $Path"
        }
        $property = $current.PSObject.Properties[$segment]
        if ($null -eq $property) {
            throw "tier report is missing $Path"
        }
        $current = $property.Value
    }
    return $current
}

function Assert-TtsS1TierReportContract {
    # Every pinned string a tier report must declare, checked before the
    # report is summarized. Case-sensitive throughout: these are identity
    # strings, not prose.
    param(
        [Parameter(Mandatory = $true)][string]$Tier,
        [Parameter(Mandatory = $true)][AllowNull()]$Report
    )
    $checks = @(
        @{ Path = 'body.verdict_view'; Expected = $script:TtsS1VerdictView; What = 'gating view' },
        @{ Path = 'body.compute_cap.rule'; Expected = $script:TtsS1ProjectionRule; What = 'compute-cap rule' },
        @{ Path = 'body.compute_cap.latency_curve.rule'; Expected = $script:TtsS1LatencyCurveRule; What = 'latency-curve rule' }
    )
    foreach ($check in $checks) {
        $observed = Get-TtsS1ReportField -Report $Report -Path $check.Path
        if ($observed -cne $check.Expected) {
            throw ("tier {0} declares {1} '{2}' at {3}, not the pre-registered '{4}'" -f `
                $Tier, $check.What, $observed, $check.Path, $check.Expected)
        }
    }
}
