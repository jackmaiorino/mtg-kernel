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
    $publicMethods[0].GetParameters().Count -ne 0 -or
    $publicMethods[0].ReturnType -ne [byte[]]) {
    throw 'producer must expose exactly one parameterless byte-array method'
}

$result = [byte[]]$publicMethods[0].Invoke($null, @())
$observed = [Text.Encoding]::UTF8.GetString($result)
$expected = '{"result_kind":"abstained","reason":"duel_surface_unavailable"}'
if ($observed -ne $expected) {
    throw "offline producer emitted unexpected output: $observed"
}
if ($result.Length -gt 128) {
    throw 'producer abstention output exceeded the fixed bound'
}

Write-Output 'MTGO_VISIBLE_DUEL_PRODUCER_BUILT_OUTPUT_V1:PASS'
