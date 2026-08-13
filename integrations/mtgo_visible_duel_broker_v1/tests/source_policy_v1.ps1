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
    'DispatchVisibleSingleBlockerStepV1',
    'execute_visible_single_blocker_step_v1|',
    'DispatchVisibleBlockerStepV1',
    'execute_visible_blocker_step_v1|',
    '--blocker-selection-sha256',
    '--model-selection-sha256',
    '--step-sha256',
    '--attacker-selection-sha256',
    '--single-blocker-selection-sha256',
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
    '857478e466fc3cb7e4473d069ec46837f95abfa137b815062da818859b237e60',
    '942508d83653f9fbc3555174102645e70a6a6d07f445debe1221504a7ba5cb96',
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
    'c0e02fec4355083d334e667151b9354558a48c78cae77630efbd6fe40e7dc547',
    'b67b5c4e31f65c81e26c04578655015812d1da56359314d57e95ac538e400233',
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
