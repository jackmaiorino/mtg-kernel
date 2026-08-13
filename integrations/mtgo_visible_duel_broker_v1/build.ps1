$ErrorActionPreference = 'Stop'

$root = $PSScriptRoot
$build = Join-Path $root 'build'
$vcvars = 'C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$netfxInclude = 'C:\Program Files (x86)\Windows Kits\NETFXSDK\4.8\Include\um'
$netfxLib = 'C:\Program Files (x86)\Windows Kits\NETFXSDK\4.8\Lib\um\x64'
foreach ($required in @($vcvars, $netfxInclude, $netfxLib)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "required build dependency missing"
    }
}
New-Item -ItemType Directory -Force -Path $build | Out-Null

$bootstrapSource = Join-Path $root 'bootstrap\VisibleDuelBootstrapV1.cpp'
$brokerSource = Join-Path $root 'broker\VisibleDuelBrokerV1.cpp'
$bootstrapOut = Join-Path $build 'mtgo_visible_duel_bootstrap_v1.dll'
$brokerOut = Join-Path $build 'mtgo_visible_duel_broker_v1.exe'
$bootstrapObject = Join-Path $build 'VisibleDuelBootstrapV1.obj'
$bootstrapImport = Join-Path $build 'VisibleDuelBootstrapV1.lib'
$brokerObject = Join-Path $build 'VisibleDuelBrokerV1.obj'

$bootstrapCommand = "call `"$vcvars`" >nul && cl /nologo /std:c++20 /permissive- /W4 /WX /EHsc /guard:cf /GS /DUNICODE /D_UNICODE /LD /Fo`"$bootstrapObject`" `"$bootstrapSource`" /I`"$netfxInclude`" /link /OUT:`"$bootstrapOut`" /IMPLIB:`"$bootstrapImport`" /LIBPATH:`"$netfxLib`" mscoree.lib"
& cmd.exe /d /c $bootstrapCommand
if ($LASTEXITCODE -ne 0) {
    throw "native bootstrap build failed"
}
$brokerCommand = "call `"$vcvars`" >nul && cl /nologo /std:c++20 /permissive- /W4 /WX /EHsc /guard:cf /GS /DUNICODE /D_UNICODE /Fo`"$brokerObject`" `"$brokerSource`" /link /OUT:`"$brokerOut`" bcrypt.lib"
& cmd.exe /d /c $brokerCommand
if ($LASTEXITCODE -ne 0) {
    throw "native broker build failed"
}

dotnet build (Join-Path $root 'synthetic_host\SyntheticManagedHostV1.csproj') -c Release
if ($LASTEXITCODE -ne 0) {
    throw "synthetic managed host build failed"
}
Write-Output 'MTGO_VISIBLE_DUEL_BROKER_BUILD_V1:PASS'
