$ErrorActionPreference = 'Stop'

$sourcePath = Join-Path $PSScriptRoot '..\VisibleDuelProducerV1.cs'
$source = [IO.File]::ReadAllText((Resolve-Path $sourcePath))

$required = @(
    'Shiny.Play.Duel.DuelScene',
    'FrameworkElement',
    'DataContext',
    'Shiny.Play.Duel.ViewModel.DuelSceneViewModel',
    'AllowedGetters.Length != 46',
    'projection_incomplete',
    'BindingFlags.Instance | BindingFlags.Public | BindingFlags.FlattenHierarchy',
    'MemoryMappedFile.OpenExisting',
    'MaximumOutputBytes',
    'IsExactChannelName',
    'TryValidateVisibleChromeProjectionV1',
    'TryReadExactPropertyV1',
    'property.GetValue(target, null)',
    'MaximumVisibleTextCharacters',
    'MaximumVisibleCollectionItems',
    'TryValidateVisibleZonesAndCardsV1',
    'TryValidateNeverEnumeratedZoneRootV1',
    'TryValidateConditionalVisibleZoneV1',
    'TryValidateVisibleBattlefieldCardV1',
    'TryValidateVisibleZoneCardV1',
    'AllowedGetters.Contains(exactKey, StringComparer.Ordinal)'
)
foreach ($marker in $required) {
    if (-not $source.Contains($marker)) {
        throw "required visible-producer marker missing: $marker"
    }
}

if (-not $source.Contains('TryValidateConditionalVisibleZoneV1(libraryZone, false, false)') -and
    -not $source.Contains('TryValidateNeverEnumeratedZoneRootV1(libraryZone)')) {
    throw 'library zone must remain non-enumerated'
}
if ($source.Contains('TryValidateEnumeratedVisibleZoneV1(libraryZone')) {
    throw 'library cards must never be enumerated'
}
if (-not $source.Contains('TryValidateConditionalVisibleZoneV1(') -or
    -not $source.Contains('handZone,') -or
    -not $source.Contains('localPlayer,')) {
    throw 'hand enumeration must remain local-player or visible-zone gated'
}

if ([regex]::Matches($source, [regex]::Escape('property.GetValue(target, null)')).Count -ne 1) {
    throw 'producer source must contain exactly one exact allowlisted getter invocation'
}

$forbidden = @(
    'ReadProcessMemory',
    'WriteProcessMemory',
    'CreateRemoteThread',
    'VirtualAllocEx',
    'Socket',
    'HttpClient',
    'WebRequest',
    'System.IO.File',
    'System.IO.Directory',
    'FileStream',
    'StreamReader',
    'StreamWriter',
    'Process.',
    'Console.',
    'Debug.',
    'Trace.',
    'GetFields(',
    'GetMembers(',
    'GetProperties(',
    'InvokeMember(',
    'Dynamic',
    'GameCard',
    'GamePlayer',
    'ModelZone',
    'CardDefinition',
    'CurrentTargetList',
    'PendingTargets',
    'ModeIDs',
    'ActionFlags'
)
foreach ($marker in $forbidden) {
    if ($source.Contains($marker)) {
        throw "forbidden visible-producer marker present: $marker"
    }
}

$getterLines = [regex]::Matches($source, '"(?:Card|DuelScene)\|[^"\r\n]+\|[^"\r\n]+"')
if ($getterLines.Count -ne 46) {
    throw "producer source must contain exactly 46 compile-time getter entries"
}

Write-Output 'MTGO_VISIBLE_DUEL_PRODUCER_SOURCE_POLICY_V1:PASS'
