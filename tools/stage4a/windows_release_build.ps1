param(
    [Parameter(Mandatory=$true)][string]$SourceRoot,
    [Parameter(Mandatory=$true)][string]$ExpectedSha,
    [string]$TargetDirectory = 'D:/cargo-target-search-sharing-allocation-20261013',
    [int]$Jobs = 4,
    [string]$LinkerPath = 'C:/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe'
)
$ErrorActionPreference = 'Stop'
$buildDirectory = Join-Path $TargetDirectory ('build-receipts/' + $ExpectedSha)
New-Item -ItemType Directory -Path $buildDirectory -Force | Out-Null
$receiptPath = Join-Path $buildDirectory 'windows-release-build.json'
$logPath = Join-Path $buildDirectory 'windows-release-build.log'
$buildTemporary = Join-Path $TargetDirectory 'build-tmp'
New-Item -ItemType Directory -Path $buildTemporary -Force | Out-Null
Set-Location -LiteralPath $SourceRoot
$actualSha = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $actualSha -ne $ExpectedSha) { throw 'Frozen source SHA mismatch.' }
if ($Jobs -lt 1 -or $Jobs -gt 4) { throw 'Cargo jobs must be within the assigned four-job limit.' }
if (!(Test-Path -LiteralPath $LinkerPath)) { throw 'Pinned MSVC linker missing.' }
$rustc = (& rustc -vV) -join "`n"
$cargo = (& cargo -V) -join "`n"
if ($rustc -notmatch '(?m)^release: 1\.94\.1$') { throw 'Pinned Rust 1.94.1 is required.' }
$env:CARGO_TARGET_DIR = $TargetDirectory
$env:CARGO_PROFILE_RELEASE_LTO = 'false'
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '16'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $LinkerPath
$env:TMP = $buildTemporary
$env:TEMP = $buildTemporary
$receipt = [ordered]@{
    schema = 'search-sharing-windows-release-build-v1'
    source_sha = $actualSha
    source_root = $SourceRoot
    target_directory = $TargetDirectory
    rustc = $rustc
    cargo = $cargo
    linker_path = $LinkerPath
    linker_version = (Get-Item -LiteralPath $LinkerPath).VersionInfo.FileVersion
    release_lto = 'false'
    release_codegen_units = 16
    cargo_jobs = $Jobs
    command = "cargo build -p mtg-kernel --bin regret_census_v1 --release --jobs $Jobs --locked"
    started_utc = (Get-Date).ToUniversalTime().ToString('o')
    status = 'building'
    stdout_log = $logPath
}
$receipt | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $receiptPath -Encoding utf8
& cargo build -p mtg-kernel --bin regret_census_v1 --release --jobs $Jobs --locked 2>&1 | Tee-Object -FilePath $logPath
$buildCode = $LASTEXITCODE
$receipt.completed_utc = (Get-Date).ToUniversalTime().ToString('o')
$receipt.exit_code = $buildCode
$receipt.status = if ($buildCode -eq 0) { 'built' } else { 'failed' }
if ($buildCode -eq 0) {
    $binaryPath = Join-Path $TargetDirectory 'release/regret_census_v1.exe'
    $pinDirectory = Join-Path 'E:/pinned-binaries' $actualSha
    $recoveryDirectory = Join-Path $TargetDirectory ('pinned-recovery/' + $actualSha)
    New-Item -ItemType Directory -Path $pinDirectory,$recoveryDirectory -Force | Out-Null
    foreach ($destination in @($pinDirectory,$recoveryDirectory)) {
        Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $destination 'regret_census_v1.exe')
    }
    $receipt.binary_sha256 = (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $receipt.binary_path = Join-Path $pinDirectory 'regret_census_v1.exe'
    $receipt.recovery_binary_path = Join-Path $recoveryDirectory 'regret_census_v1.exe'
    $receipt | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $pinDirectory 'windows-release-build.json') -Encoding utf8
}
$receipt | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $receiptPath -Encoding utf8
exit $buildCode
