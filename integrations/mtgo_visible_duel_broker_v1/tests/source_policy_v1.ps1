$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$bootstrap = [IO.File]::ReadAllText((Join-Path $root 'bootstrap\VisibleDuelBootstrapV1.cpp'))
$broker = [IO.File]::ReadAllText((Join-Path $root 'broker\VisibleDuelBrokerV1.cpp'))
$buildScript = [IO.File]::ReadAllText((Join-Path $root 'build.ps1'))

foreach ($required in @(
    'ExecuteInDefaultAppDomain',
    'IsLoaded(GetCurrentProcess()',
    'ExportVisibleDecisionOrAbstainV1',
    'DispatchVisibleAttackerStepV1',
    'execute_visible_attacker_step_v1|',
    '--attacker-selection-sha256',
    '--candidate-count',
    '--desired-mask',
    '--plan-sha256',
    'CreateFileMappingW',
    'CreateRemoteThread',
    'WriteProcessMemory',
    'AllowedResultV1',
    'StrictValidatorAcceptsV1',
    'CreateProcessW',
    'SecureZeroMemory',
    'synthetic_managed_host_v1.exe',
    'target_not_synthetic_host',
    'MTGO_LIVE_PINNED_V1',
    'MTGO_LIVE_DISPATCH_ADMITTED_V1',
    'bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92',
    '72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e',
    '071338a98d845d5c8db6ebd2f3c847e38ad548f50ba11d2a36973438cdec2ea8',
    'f3fef1adfd5b1b6d25a5db577f9a1b184c8b91bb98f19a13428c669266c20dc8',
    '1d764382d56fe27aa845acf10b92ee8b9effd79d161baeaace1294a2d01c8c9b',
    'aac76ba607b8026346d3bf84787b687270b099c0ef6c7a483980b5250a3356da',
    'd3236104719a9c064e392bf1061caae407a502fe3b880725e32568999ed590ec',
    'WotC.MtGO.Client.Model.Reference.dll',
    'AuthenticodeValidV1',
    'ExactPinnedVersionV1',
    'ExactlyOneMtgoProcessV1',
    'live_identity_pre',
    'live_identity_post',
    'live_dispatch_not_admitted'
)) {
    if (-not ($bootstrap.Contains($required) -or $broker.Contains($required))) {
        throw "required broker marker missing"
    }
}
foreach ($requiredBuild in @(
    'mtgo_visible_duel_live_dispatch_broker_v1.exe',
    '/DMTGO_LIVE_PINNED_V1 /DMTGO_LIVE_DISPATCH_ADMITTED_V1',
    'a02262a54f1b38c449a8e2f450a39955326aab20573d40d313a88f025b9ab099',
    'native live observe-only broker differs from its release pin'
)) {
    if (-not $buildScript.Contains($requiredBuild)) {
        throw "required dispatch build marker missing"
    }
}
if (-not $broker.Contains('#if defined(MTGO_LIVE_PINNED_V1) && !defined(MTGO_LIVE_DISPATCH_ADMITTED_V1)')) {
    throw "observe-only live dispatch rejection is not compile-separated"
}
foreach ($forbidden in @(
    'ReadProcessMemory',
    'MiniDump',
    'DebugActiveProcess',
    'AdjustTokenPrivileges',
    'SeDebugPrivilege',
    'WinHttp',
    'WinSock',
    'URLDownload',
    'SendInput',
    'SetCursorPos',
    'PostMessage',
    'SendMessage',
    'std::cout',
    'std::wcerr'
)) {
    if ($bootstrap.Contains($forbidden) -or $broker.Contains($forbidden)) {
        throw "forbidden broker marker present"
    }
}

Write-Output 'MTGO_VISIBLE_DUEL_BROKER_SOURCE_POLICY_V1:PASS'
