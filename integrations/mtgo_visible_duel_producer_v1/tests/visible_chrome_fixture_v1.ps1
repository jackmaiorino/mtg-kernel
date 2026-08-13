$ErrorActionPreference = 'Stop'

$project = Join-Path $PSScriptRoot 'fixtures\ChromeHost\ChromeHost.csproj'
dotnet build $project -c Release
if ($LASTEXITCODE -ne 0) {
    throw 'visible chrome fixture build failed'
}
$hostPath = (Resolve-Path (Join-Path $PSScriptRoot 'fixtures\ChromeHost\bin\Release\net472\visible_chrome_fixture_host_v1.exe')).Path
$encoded = & $hostPath
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($encoded)) {
    throw "visible chrome fixture failed with exit code $LASTEXITCODE"
}
$json = [Text.Encoding]::UTF8.GetString(
    [Convert]::FromBase64String(($encoded | Select-Object -Last 1))
)
$validator = Get-Command check_mtgo_visible_duel_producer_result_v1.exe -ErrorAction SilentlyContinue |
    Select-Object -First 1 -ExpandProperty Source
if ([string]::IsNullOrWhiteSpace($validator) -or
    -not (Test-Path -LiteralPath $validator)) {
    throw 'strict Rust producer-result validator is not available on PATH'
}
$json | & $validator
if ($LASTEXITCODE -ne 0) {
    throw 'visible chrome fixture output failed strict Rust validation'
}

Write-Output 'MTGO_VISIBLE_DUEL_PRODUCER_VISIBLE_CHROME_FIXTURE_V1:PASS'
