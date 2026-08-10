# Gate A3 comparison: file-for-file byte identity between the legacy-side
# and launcher-side store trees. On mismatch, reports every differing
# relative path and the first divergent byte offset of the first mismatch,
# so a path-embedding difference (store parent appearing inside record
# bytes) is distinguishable from real behavioral divergence at a glance.
param(
    [string]$LegacyRoot = 'D:\mtg-kernel-oppoint-a3-legacy\proc-0',
    [string]$LauncherRoot = 'D:\mtg-kernel-oppoint-a3-launcher\proc-0'
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-RelativeFileMap {
    param([string]$Root)
    $map = @{}
    foreach ($file in Get-ChildItem -LiteralPath $Root -Recurse -File) {
        $relative = $file.FullName.Substring($Root.Length).TrimStart('\')
        $map[$relative] = $file.FullName
    }
    return $map
}

$legacy = Get-RelativeFileMap -Root (Resolve-Path -LiteralPath $LegacyRoot).Path
$launcher = Get-RelativeFileMap -Root (Resolve-Path -LiteralPath $LauncherRoot).Path

$problems = @()
foreach ($relative in $legacy.Keys) {
    if (-not $launcher.ContainsKey($relative)) { $problems += "only in legacy: $relative" }
}
foreach ($relative in $launcher.Keys) {
    if (-not $legacy.ContainsKey($relative)) { $problems += "only in launcher: $relative" }
}
$firstDivergence = $null
foreach ($relative in ($legacy.Keys | Where-Object { $launcher.ContainsKey($_) } | Sort-Object)) {
    $legacyHash = (Get-FileHash -LiteralPath $legacy[$relative] -Algorithm SHA256).Hash
    $launcherHash = (Get-FileHash -LiteralPath $launcher[$relative] -Algorithm SHA256).Hash
    if ($legacyHash -cne $launcherHash) {
        $problems += "content differs: $relative"
        if ($null -eq $firstDivergence) {
            $legacyBytes = [IO.File]::ReadAllBytes($legacy[$relative])
            $launcherBytes = [IO.File]::ReadAllBytes($launcher[$relative])
            $limit = [Math]::Min($legacyBytes.Length, $launcherBytes.Length)
            $offset = 0
            while ($offset -lt $limit -and $legacyBytes[$offset] -eq $launcherBytes[$offset]) { $offset += 1 }
            $contextStart = [Math]::Max(0, $offset - 40)
            $contextLength = [Math]::Min(80, $limit - $contextStart)
            $legacyContext = [Text.Encoding]::ASCII.GetString($legacyBytes, $contextStart, $contextLength) -replace '[^\x20-\x7e]', '.'
            $launcherContext = [Text.Encoding]::ASCII.GetString($launcherBytes, $contextStart, $contextLength) -replace '[^\x20-\x7e]', '.'
            $firstDivergence = "first divergence in ${relative} at byte ${offset} (lengths $($legacyBytes.Length)/$($launcherBytes.Length))`n  legacy:   ...$legacyContext...`n  launcher: ...$launcherContext..."
        }
    }
}

if ($problems.Count -eq 0) {
    Write-Output ("A3 PASS: {0} files byte-identical" -f $legacy.Keys.Count)
}
else {
    foreach ($problem in $problems) { Write-Output "A3 MISMATCH: $problem" }
    if ($null -ne $firstDivergence) { Write-Output $firstDivergence }
    throw ("A3 FAIL: {0} problem(s)" -f $problems.Count)
}
