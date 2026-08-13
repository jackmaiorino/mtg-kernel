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
if ($publicMethods.Count -ne 1 -or
    $publicMethods[0].Name -ne 'ExportVisibleDecisionOrAbstainV1' -or
    $publicMethods[0].GetParameters().Count -ne 1 -or
    $publicMethods[0].GetParameters()[0].ParameterType -ne [string] -or
    $publicMethods[0].ReturnType -ne [int]) {
    throw 'producer must expose exactly one string-to-integer broker method'
}

$invalidStatus = [int]$publicMethods[0].Invoke($null, @('invalid'))
if ($invalidStatus -ne 2) {
    throw 'invalid broker channel name was not rejected'
}

$channelName = 'Local\mtgkernel_mtgo_visible_v1_' + ('a' * 64)
$capacity = 1048576
$channel = [IO.MemoryMappedFiles.MemoryMappedFile]::CreateNew($channelName, $capacity)
$view = $channel.CreateViewAccessor(0, $capacity, [IO.MemoryMappedFiles.MemoryMappedFileAccess]::ReadWrite)
try {
    $status = [int]$publicMethods[0].Invoke($null, @($channelName))
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
