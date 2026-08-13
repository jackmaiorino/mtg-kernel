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
    'IsExactChannelName'
)
foreach ($marker in $required) {
    if (-not $source.Contains($marker)) {
        throw "required visible-producer marker missing: $marker"
    }
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
