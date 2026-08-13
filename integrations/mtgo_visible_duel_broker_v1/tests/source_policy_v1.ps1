$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$bootstrap = [IO.File]::ReadAllText((Join-Path $root 'bootstrap\VisibleDuelBootstrapV1.cpp'))
$broker = [IO.File]::ReadAllText((Join-Path $root 'broker\VisibleDuelBrokerV1.cpp'))

foreach ($required in @(
    'ExecuteInDefaultAppDomain',
    'IsLoaded(GetCurrentProcess()',
    'ExportVisibleDecisionOrAbstainV1',
    'CreateFileMappingW',
    'CreateRemoteThread',
    'WriteProcessMemory',
    'AllowedResultV1',
    'SecureZeroMemory',
    'synthetic_managed_host_v1.exe',
    'target_not_synthetic_host'
)) {
    if (-not ($bootstrap.Contains($required) -or $broker.Contains($required))) {
        throw "required broker marker missing"
    }
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
