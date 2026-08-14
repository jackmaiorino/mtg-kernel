$ErrorActionPreference = 'Stop'

$root = $PSScriptRoot
$build = if ([string]::IsNullOrWhiteSpace($env:MTGO_VISIBLE_DUEL_BROKER_BUILD_DIR)) {
    Join-Path $root 'build'
} else {
    [IO.Path]::GetFullPath($env:MTGO_VISIBLE_DUEL_BROKER_BUILD_DIR)
}
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
$liveBrokerOut = Join-Path $build 'mtgo_visible_duel_live_broker_v1.exe'
$liveDispatchBrokerOut = Join-Path $build 'mtgo_visible_duel_live_dispatch_broker_v1.exe'
$bootstrapObject = Join-Path $build 'VisibleDuelBootstrapV1.obj'
$bootstrapImport = Join-Path $build 'VisibleDuelBootstrapV1.lib'
$brokerObject = Join-Path $build 'VisibleDuelBrokerV1.obj'
$liveBrokerObject = Join-Path $build 'VisibleDuelLiveBrokerV1.obj'
$liveDispatchBrokerObject = Join-Path $build 'VisibleDuelLiveDispatchBrokerV1.obj'

$bootstrapCommand = "call `"$vcvars`" >nul && cl /nologo /std:c++20 /permissive- /W4 /WX /EHsc /guard:cf /GS /ZH:SHA_256 /DUNICODE /D_UNICODE /LD /Fo`"$bootstrapObject`" `"$bootstrapSource`" /I`"$netfxInclude`" /link /Brepro /OUT:`"$bootstrapOut`" /IMPLIB:`"$bootstrapImport`" /LIBPATH:`"$netfxLib`" mscoree.lib"
& cmd.exe /d /c $bootstrapCommand
if ($LASTEXITCODE -ne 0) {
    throw "native bootstrap build failed"
}
$brokerCommand = "call `"$vcvars`" >nul && cl /nologo /std:c++20 /permissive- /W4 /WX /EHsc /guard:cf /GS /ZH:SHA_256 /DUNICODE /D_UNICODE /Fo`"$brokerObject`" `"$brokerSource`" /link /Brepro /OUT:`"$brokerOut`" bcrypt.lib"
& cmd.exe /d /c $brokerCommand
if ($LASTEXITCODE -ne 0) {
    throw "native broker build failed"
}
$liveBrokerCommand = "call `"$vcvars`" >nul && cl /nologo /std:c++20 /permissive- /W4 /WX /EHsc /guard:cf /GS /ZH:SHA_256 /DUNICODE /D_UNICODE /DMTGO_LIVE_PINNED_V1 /Fo`"$liveBrokerObject`" `"$brokerSource`" /link /Brepro /OUT:`"$liveBrokerOut`" bcrypt.lib version.lib wintrust.lib"
& cmd.exe /d /c $liveBrokerCommand
if ($LASTEXITCODE -ne 0) {
    throw "native live broker build failed"
}
$liveDispatchBrokerCommand = "call `"$vcvars`" >nul && cl /nologo /std:c++20 /permissive- /W4 /WX /EHsc /guard:cf /GS /ZH:SHA_256 /DUNICODE /D_UNICODE /DMTGO_LIVE_PINNED_V1 /DMTGO_LIVE_DISPATCH_ADMITTED_V1 /Fo`"$liveDispatchBrokerObject`" `"$brokerSource`" /link /Brepro /OUT:`"$liveDispatchBrokerOut`" bcrypt.lib version.lib wintrust.lib"
& cmd.exe /d /c $liveDispatchBrokerCommand
if ($LASTEXITCODE -ne 0) {
    throw "native live dispatch broker build failed"
}
$expectedLiveBrokerSha256 = 'e83e1f08260cdbd80e68527964de54afd2774beed8f344b03f609fdd75a34fab'
$expectedLiveDispatchBrokerSha256 = '95dcfe3eb38006e8dd26800776ef2e329aef1a9dbf265ba5a0bfe179247b9e49'
$observedLiveBrokerSha256 = (Get-FileHash -LiteralPath $liveBrokerOut -Algorithm SHA256).Hash.ToLowerInvariant()
$observedLiveDispatchBrokerSha256 = (Get-FileHash -LiteralPath $liveDispatchBrokerOut -Algorithm SHA256).Hash.ToLowerInvariant()
if ($observedLiveBrokerSha256 -ne $expectedLiveBrokerSha256) {
    throw "native live observe-only broker differs from its release pin"
}
if ($observedLiveDispatchBrokerSha256 -ne $expectedLiveDispatchBrokerSha256) {
    throw "native live dispatch broker differs from its release pin"
}

dotnet build (Join-Path $root 'synthetic_host\SyntheticManagedHostV1.csproj') -c Release
if ($LASTEXITCODE -ne 0) {
    throw "synthetic managed host build failed"
}
Write-Output 'MTGO_VISIBLE_DUEL_BROKER_BUILD_V1:PASS'
