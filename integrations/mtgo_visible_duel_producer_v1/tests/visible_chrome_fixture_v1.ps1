$ErrorActionPreference = 'Stop'

$project = Join-Path $PSScriptRoot 'fixtures\ChromeHost\ChromeHost.csproj'
dotnet build $project -c Release
if ($LASTEXITCODE -ne 0) {
    throw 'visible chrome fixture build failed'
}
$hostPath = (Resolve-Path (Join-Path $PSScriptRoot 'fixtures\ChromeHost\bin\Release\net472\visible_chrome_fixture_host_v1.exe')).Path
$process = Start-Process -FilePath $hostPath -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) {
    throw "visible chrome fixture failed with exit code $($process.ExitCode)"
}

Write-Output 'MTGO_VISIBLE_DUEL_PRODUCER_VISIBLE_CHROME_FIXTURE_V1:PASS'
