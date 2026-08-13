$ErrorActionPreference = 'Stop'

$sourcePath = Join-Path $PSScriptRoot '..\VisibleDuelProducerV1.cs'
$source = [IO.File]::ReadAllText((Resolve-Path $sourcePath))
$sanitizerPath = Join-Path $PSScriptRoot '..\SanitizedVisibleDecisionV1.cs'
$dispatchPath = Join-Path $PSScriptRoot '..\SealedVisibleActionDispatchV1.cs'
$completeSource = $source + "`n" +
    [IO.File]::ReadAllText((Resolve-Path $sanitizerPath)) + "`n" +
    [IO.File]::ReadAllText((Resolve-Path $dispatchPath))

$required = @(
    'Shiny.Play.Duel.DuelScene',
    'FrameworkElement',
    'DataContext',
    'Shiny.Play.Duel.ViewModel.DuelSceneViewModel',
    'AllowedGetters.Length != 80',
    'PrivateVisibleActionJoinGetters.Length != 33',
    'projection_incomplete',
    'BindingFlags.Instance | BindingFlags.Public | BindingFlags.FlattenHierarchy',
    'MemoryMappedFile.OpenExisting',
    'MaximumOutputBytes',
    'IsExactChannelName',
    'TryValidateVisibleChromeProjectionV1',
    'TryParseVisibleTurnV1',
    'TryRequireNoUnrepresentedVisiblePlayerCountersV1',
    'TryRequireNoVisibleCompanionPanelV1',
    'TryRequireNoUnrepresentedVisibleModalSurfaceV1',
    'TryRequireSupportedDuelVariantV1',
    'TryRequireNoUnrepresentedVisibleCardStateV1',
    'TryReadExactPropertyV1',
    'property.GetValue(target, null)',
    'MaximumVisibleTextCharacters',
    'MaximumVisibleCollectionItems',
    'TryValidateVisibleZonesAndCardsV1',
    'TryValidateNeverEnumeratedZoneRootV1',
    'TryValidateConditionalVisibleZoneV1',
    'TryValidateVisibleBattlefieldCardV1',
    'TryValidateVisibleZoneCardV1',
    'TryValidateVisibleInitiativeShieldZoneV1',
    'TryMapVisibleInitiativeHolderV1',
    'CLBInitiativeEmblem',
    'seatedCount != seated.Shields.Count',
    'visibleInitiative != null',
    'AllowedGetters.Contains(exactKey, StringComparer.Ordinal)',
    'TryValidatePrivateVisibleCardActionJoinsV1',
    'TryValidatePrivateVisibleActionV1',
    'TryRequireBasicVisibleCardActionMenuShapeV1',
    'TryRequireNoUnrepresentedVisibleCardActionModalV1',
    '"ManaButtons"',
    '"NumberEntry"',
    'out object? actionFlagsValue',
    'actionFlags != 1u',
    'TryReadExactPrivateVisibleActionPropertyV1',
    'PrivateVisibleActionJoinGetters.Contains(',
    'DispatchSelectedVisibleActionV1',
    'execute_visible_action_v1|',
    'ExpectedDecisionSha256',
    'ExecuteAction',
    'ConditionalWeakTable<object, HashSet<string>>',
    'DispatchedVisibleDecisionsByGameV1',
    '!dispatched.Add(request.ExpectedDecisionSha256)'
)
foreach ($marker in $required) {
    if (-not $completeSource.Contains($marker)) {
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

if ([regex]::Matches($source, [regex]::Escape('property.GetValue(target, null)')).Count -ne 2) {
    throw 'producer source must contain exactly two exact allowlisted getter invocations'
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
    'HiddenActions',
    'GlobalActions',
    'CurrentTurn'
)
foreach ($marker in $forbidden) {
    if ($completeSource.Contains($marker)) {
        throw "forbidden visible-producer marker present: $marker"
    }
}

if ($completeSource.Contains('["action_flags"]') -or
    $completeSource.Contains('"action_flags"')) {
    throw 'private action flags must never enter the serialized visible schema'
}

$publicStart = $source.IndexOf('private static readonly string[] AllowedGetters')
$privateStart = $source.IndexOf('private static readonly string[] PrivateVisibleActionJoinGetters')
if ($publicStart -lt 0 -or $privateStart -le $publicStart) {
    throw 'producer getter arrays are missing or out of order'
}
$publicGetterSource = $source.Substring($publicStart, $privateStart - $publicStart)
$getterLines = [regex]::Matches($publicGetterSource, '"(?:Card|DuelScene)\|[^"\r\n]+\|[^"\r\n]+"')
if ($getterLines.Count -ne 80) {
    throw "producer source must contain exactly 80 compile-time getter entries"
}

$privateEnd = $source.IndexOf('};', $privateStart)
if ($privateEnd -lt 0) {
    throw 'private action join getter array is unterminated'
}
$privateGetterSource = $source.Substring($privateStart, $privateEnd - $privateStart)
$privateGetterLines = [regex]::Matches(
    $privateGetterSource,
    '"(?:DuelScene|WotC\.MtGO\.Client\.Model\.Reference)\|[^"\r\n]+\|[^"\r\n]+"'
)
if ($privateGetterLines.Count -ne 33) {
    throw 'producer source must contain exactly 33 private visible-source join getters'
}

if (-not $source.Contains('if (localPlayer)') -or
    -not $source.Contains('seatedPlayerVisibleActionCards.Add(card)') -or
    -not $source.Contains('TryValidatePrivateVisibleCardActionJoinsV1(') -or
    $source.Contains("TryValidatePrivateVisibleCardActionJoinsV1(`r`n                battlefieldCards)") -or
    $source.Contains("TryValidatePrivateVisibleCardActionJoinsV1(`n                battlefieldCards)")) {
    throw 'private card-action joins must remain bound to seated-player visible cards'
}

Write-Output 'MTGO_VISIBLE_DUEL_PRODUCER_SOURCE_POLICY_V1:PASS'
