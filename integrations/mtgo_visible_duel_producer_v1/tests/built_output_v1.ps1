param(
    [Parameter(Mandatory = $true)]
    [string] $AssemblyPath
)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $AssemblyPath).Path
$assembly = [Reflection.Assembly]::LoadFile($resolved)
$expectedReferences = @(
    'mscorlib',
    'PresentationCore',
    'PresentationFramework',
    'System.Core',
    'System.Runtime.Serialization',
    'WindowsBase'
)
$actualReferences = @($assembly.GetReferencedAssemblies().Name | Sort-Object -Unique)
if (Compare-Object $expectedReferences $actualReferences) {
    throw "producer assembly reference set changed: $($actualReferences -join ',')"
}

$type = $assembly.GetType(
    'MtgKernel.Mtgo.VisibleDuelProducer.V1.VisibleDuelProducerV1',
    $true,
    $false
)
$publicMethods = @(
    $type.GetMethods([Reflection.BindingFlags]'Public,Static,DeclaredOnly') |
        Where-Object { -not $_.IsSpecialName }
)
if ($publicMethods.Count -ne 5 -or
    (Compare-Object @(
        'DispatchSelectedVisibleActionV1',
        'DispatchVisibleAttackerStepV1',
        'DispatchVisibleBlockerStepV1',
        'DispatchVisibleSingleBlockerStepV1',
        'ExportVisibleDecisionOrAbstainV1'
    ) @($publicMethods.Name | Sort-Object)) -or
    @($publicMethods | Where-Object {
        $_.GetParameters().Count -ne 1 -or
        $_.GetParameters()[0].ParameterType -ne [string] -or
        $_.ReturnType -ne [int]
    }).Count -ne 0) {
    throw 'producer must expose exactly five string-to-integer broker methods'
}

$exportMethod = $publicMethods | Where-Object Name -eq 'ExportVisibleDecisionOrAbstainV1'
$dispatchMethod = $publicMethods | Where-Object Name -eq 'DispatchSelectedVisibleActionV1'
$attackerDispatchMethod = $publicMethods | Where-Object Name -eq 'DispatchVisibleAttackerStepV1'
$blockerDispatchMethod = $publicMethods | Where-Object Name -eq 'DispatchVisibleBlockerStepV1'
$singleBlockerDispatchMethod = $publicMethods | Where-Object Name -eq 'DispatchVisibleSingleBlockerStepV1'
$invalidStatus = [int]$exportMethod.Invoke($null, @('invalid'))
if ($invalidStatus -ne 2) {
    throw 'invalid broker channel name was not rejected'
}
if ([int]$dispatchMethod.Invoke($null, @('invalid')) -ne 2) {
    throw 'invalid dispatch channel name was not rejected'
}
if ([int]$attackerDispatchMethod.Invoke($null, @('invalid')) -ne 2) {
    throw 'invalid attacker dispatch channel name was not rejected'
}
if ([int]$blockerDispatchMethod.Invoke($null, @('invalid')) -ne 2) {
    throw 'invalid blocker dispatch channel name was not rejected'
}
if ([int]$singleBlockerDispatchMethod.Invoke($null, @('invalid')) -ne 2) {
    throw 'invalid single-blocker dispatch channel name was not rejected'
}

$channelName = 'Local\mtgkernel_mtgo_visible_v1_' + ('a' * 64)
$capacity = 1048576
$channel = [IO.MemoryMappedFiles.MemoryMappedFile]::CreateNew($channelName, $capacity)
$view = $channel.CreateViewAccessor(0, $capacity, [IO.MemoryMappedFiles.MemoryMappedFileAccess]::ReadWrite)
try {
    $status = [int]$exportMethod.Invoke($null, @($channelName))
    if ($status -ne 0) {
        throw "offline producer returned transport status $status"
    }
    $length = $view.ReadInt32(0)
    $schema = $view.ReadInt32(4)
    if ($schema -ne 1 -or $length -le 0 -or $length -gt 128) {
        throw "producer output header was invalid: schema=$schema length=$length"
    }
    $result = New-Object byte[] $length
    [void]$view.ReadArray(8, $result, 0, $length)
    $observed = [Text.Encoding]::UTF8.GetString($result)
    $expected = '{"result_kind":"abstained","reason":"duel_surface_unavailable"}'
    if ($observed -ne $expected) {
        throw "offline producer emitted unexpected output: $observed"
    }
} finally {
    $view.Dispose()
    $channel.Dispose()
}

Write-Output 'MTGO_VISIBLE_DUEL_PRODUCER_BUILT_OUTPUT_V1:PASS'
