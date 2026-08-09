[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$OutputDirectory,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$ExpectedProductVersion,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9A-Fa-f]{64}$')]
    [string]$ExpectedExecutableSha256,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9A-Fa-f]{40}$')]
    [string]$ExpectedSignerThumbprint,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$ExpectedSignerSubject,

    [Parameter(Mandatory = $true)]
    [ValidateRange(96, 480)]
    [int]$ExpectedDpi,

    [ValidateNotNullOrEmpty()]
    [string]$ExpectedWindowTitle = 'Magic: The Gathering Online',

    [ValidateSet('MainClient', 'ForegroundSpectatorGame', 'ForegroundSolitaireGame')]
    [string]$TargetWindowMode = 'MainClient',

    [ValidateSet('Standard', 'Pioneer', 'Modern', 'Legacy', 'Vintage', 'Pauper', 'Freeform')]
    [string]$ExpectedGameFormat
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# This script creates a local calibration preview only. Its output is never
# decision evidence and is never safe for OCR, policy scoring, or input.

Add-Type -AssemblyName System.Drawing

if (-not ('MtgoVisiblePreviewNativeV1' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public sealed class MtgoSignedRectV1
{
    public int Left { get; set; }
    public int Top { get; set; }
    public int Right { get; set; }
    public int Bottom { get; set; }
    public long Width { get { return (long)Right - Left; } }
    public long Height { get { return (long)Bottom - Top; } }
}

public sealed class MtgoMonitorRecordV1
{
    public long Handle { get; set; }
    public string DeviceName { get; set; }
    public bool IsPrimary { get; set; }
    public MtgoSignedRectV1 Bounds { get; set; }
    public MtgoSignedRectV1 WorkArea { get; set; }
}

public sealed class MtgoOcclusionAuditV1
{
    public bool TargetFound { get; set; }
    public int WindowsExaminedAboveTarget { get; set; }
    public int Intersections { get; set; }
}

public sealed class MtgoCursorRecordV1
{
    public bool IsShowing { get; set; }
    public int X { get; set; }
    public int Y { get; set; }
    public bool IsInsideCrop { get; set; }
}

public static class MtgoVisiblePreviewNativeV1
{
    private const uint GA_ROOT = 2;
    private const uint GW_HWNDNEXT = 2;
    private const uint DWMWA_EXTENDED_FRAME_BOUNDS = 9;
    private const uint DWMWA_CLOAKED = 14;
    private const uint MONITORINFOF_PRIMARY = 1;
    private const uint CURSOR_SHOWING = 1;

    private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    private delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData);

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int X;
        public int Y;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct MONITORINFOEX
    {
        public int Size;
        public RECT Monitor;
        public RECT WorkArea;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string DeviceName;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct CURSORINFO
    {
        public int Size;
        public uint Flags;
        public IntPtr CursorHandle;
        public POINT ScreenPosition;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);

    [DllImport("user32.dll")]
    private static extern IntPtr GetThreadDpiAwarenessContext();

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool AreDpiAwarenessContextsEqual(IntPtr first, IntPtr second);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool IsWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool IsIconic(IntPtr hWnd);

    [DllImport("user32.dll")]
    private static extern IntPtr GetAncestor(IntPtr hWnd, uint flags);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern int GetWindowTextLengthW(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int GetWindowTextW(IntPtr hWnd, StringBuilder text, int maxCount);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetClientRect(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ClientToScreen(IntPtr hWnd, ref POINT point);

    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr clipRect, MonitorEnumProc callback, IntPtr data);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetMonitorInfoW(IntPtr hMonitor, ref MONITORINFOEX info);

    [DllImport("user32.dll")]
    private static extern IntPtr GetTopWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    private static extern IntPtr GetWindow(IntPtr hWnd, uint command);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetWindowDisplayAffinity(IntPtr hWnd, out uint affinity);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetCursorInfo(ref CURSORINFO cursorInfo);

    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr hWnd, uint attribute, out RECT value, int size);

    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr hWnd, uint attribute, out int value, int size);

    [DllImport("dwmapi.dll")]
    public static extern int DwmIsCompositionEnabled([MarshalAs(UnmanagedType.Bool)] out bool enabled);

    [DllImport("dwmapi.dll")]
    public static extern int DwmFlush();

    public static IntPtr EnterPerMonitorV2()
    {
        IntPtr expected = new IntPtr(-4);
        IntPtr previous = SetThreadDpiAwarenessContext(expected);
        if (previous == IntPtr.Zero)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_DPI_CONTEXT_SET_FAILED:" + Marshal.GetLastWin32Error());
        }
        if (!AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), expected))
        {
            SetThreadDpiAwarenessContext(previous);
            throw new InvalidOperationException("MTGO_PREVIEW_DPI_CONTEXT_NOT_PER_MONITOR_V2");
        }
        return previous;
    }

    public static void RestoreDpiContext(IntPtr previous)
    {
        if (previous != IntPtr.Zero && SetThreadDpiAwarenessContext(previous) == IntPtr.Zero)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_DPI_CONTEXT_RESTORE_FAILED:" + Marshal.GetLastWin32Error());
        }
    }

    public static uint ProcessIdForWindow(IntPtr hWnd)
    {
        uint processId;
        if (GetWindowThreadProcessId(hWnd, out processId) == 0 || processId == 0)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_WINDOW_PID_QUERY_FAILED:" + Marshal.GetLastWin32Error());
        }
        return processId;
    }

    public static bool IsRootWindow(IntPtr hWnd)
    {
        return GetAncestor(hWnd, GA_ROOT) == hWnd;
    }

    public static string WindowTitle(IntPtr hWnd)
    {
        int length = GetWindowTextLengthW(hWnd);
        StringBuilder buffer = new StringBuilder(length + 1);
        int copied = GetWindowTextW(hWnd, buffer, buffer.Capacity);
        if (copied == 0 && length != 0)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_WINDOW_TITLE_QUERY_FAILED:" + Marshal.GetLastWin32Error());
        }
        return buffer.ToString();
    }

    public static IntPtr[] VisibleTopLevelWindowsForProcess(uint processId)
    {
        List<IntPtr> windows = new List<IntPtr>();
        bool ok = EnumWindows(delegate(IntPtr hWnd, IntPtr ignored)
        {
            uint candidateProcessId;
            GetWindowThreadProcessId(hWnd, out candidateProcessId);
            if (candidateProcessId == processId && IsWindowVisible(hWnd))
            {
                windows.Add(hWnd);
            }
            return true;
        }, IntPtr.Zero);
        if (!ok)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_WINDOW_ENUMERATION_FAILED:" + Marshal.GetLastWin32Error());
        }
        return windows.ToArray();
    }

    public static MtgoSignedRectV1 ClientBounds(IntPtr hWnd)
    {
        RECT client;
        if (!GetClientRect(hWnd, out client))
        {
            throw new InvalidOperationException("MTGO_PREVIEW_CLIENT_RECT_QUERY_FAILED:" + Marshal.GetLastWin32Error());
        }
        POINT topLeft = new POINT { X = client.Left, Y = client.Top };
        POINT bottomRight = new POINT { X = client.Right, Y = client.Bottom };
        if (!ClientToScreen(hWnd, ref topLeft) || !ClientToScreen(hWnd, ref bottomRight))
        {
            throw new InvalidOperationException("MTGO_PREVIEW_CLIENT_TO_SCREEN_FAILED:" + Marshal.GetLastWin32Error());
        }
        return NewRect(topLeft.X, topLeft.Y, bottomRight.X, bottomRight.Y);
    }

    public static MtgoSignedRectV1 ExtendedFrameBounds(IntPtr hWnd)
    {
        RECT rect;
        int result = DwmGetWindowAttribute(hWnd, DWMWA_EXTENDED_FRAME_BOUNDS, out rect, Marshal.SizeOf(typeof(RECT)));
        if (result < 0)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_EXTENDED_BOUNDS_QUERY_FAILED:" + result);
        }
        return NewRect(rect.Left, rect.Top, rect.Right, rect.Bottom);
    }

    public static int CloakedState(IntPtr hWnd)
    {
        int state;
        int result = DwmGetWindowAttribute(hWnd, DWMWA_CLOAKED, out state, sizeof(int));
        if (result < 0)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_CLOAK_QUERY_FAILED:" + result);
        }
        return state;
    }

    public static MtgoMonitorRecordV1[] Monitors()
    {
        List<MtgoMonitorRecordV1> monitors = new List<MtgoMonitorRecordV1>();
        bool ok = EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero,
            delegate(IntPtr handle, IntPtr ignoredHdc, ref RECT ignoredRect, IntPtr ignoredData)
            {
                MONITORINFOEX info = new MONITORINFOEX();
                info.Size = Marshal.SizeOf(typeof(MONITORINFOEX));
                if (!GetMonitorInfoW(handle, ref info))
                {
                    throw new InvalidOperationException("MTGO_PREVIEW_MONITOR_INFO_QUERY_FAILED:" + Marshal.GetLastWin32Error());
                }
                monitors.Add(new MtgoMonitorRecordV1
                {
                    Handle = handle.ToInt64(),
                    DeviceName = info.DeviceName,
                    IsPrimary = (info.Flags & MONITORINFOF_PRIMARY) != 0,
                    Bounds = NewRect(info.Monitor.Left, info.Monitor.Top, info.Monitor.Right, info.Monitor.Bottom),
                    WorkArea = NewRect(info.WorkArea.Left, info.WorkArea.Top, info.WorkArea.Right, info.WorkArea.Bottom)
                });
                return true;
            }, IntPtr.Zero);
        if (!ok)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_MONITOR_ENUMERATION_FAILED:" + Marshal.GetLastWin32Error());
        }
        return monitors.ToArray();
    }

    public static bool Intersects(MtgoSignedRectV1 first, MtgoSignedRectV1 second)
    {
        return first.Left < second.Right && first.Right > second.Left &&
               first.Top < second.Bottom && first.Bottom > second.Top;
    }

    public static bool Contains(MtgoSignedRectV1 outer, MtgoSignedRectV1 inner)
    {
        return inner.Left >= outer.Left && inner.Top >= outer.Top &&
               inner.Right <= outer.Right && inner.Bottom <= outer.Bottom;
    }

    public static MtgoOcclusionAuditV1 AuditOcclusion(IntPtr target, MtgoSignedRectV1 crop)
    {
        IntPtr current = GetTopWindow(IntPtr.Zero);
        int examined = 0;
        int intersections = 0;
        bool found = false;
        int guard = 0;
        while (current != IntPtr.Zero && guard++ < 10000)
        {
            if (current == target)
            {
                found = true;
                break;
            }
            if (IsWindowVisible(current) && !IsIconic(current) && CloakedState(current) == 0)
            {
                examined++;
                MtgoSignedRectV1 bounds = ExtendedFrameBounds(current);
                if (bounds.Width > 0 && bounds.Height > 0 && Intersects(bounds, crop))
                {
                    intersections++;
                }
            }
            current = GetWindow(current, GW_HWNDNEXT);
        }
        if (guard >= 10000)
        {
            throw new InvalidOperationException("MTGO_PREVIEW_Z_ORDER_GUARD_EXCEEDED");
        }
        return new MtgoOcclusionAuditV1
        {
            TargetFound = found,
            WindowsExaminedAboveTarget = examined,
            Intersections = intersections
        };
    }

    public static MtgoCursorRecordV1 Cursor(MtgoSignedRectV1 crop)
    {
        CURSORINFO info = new CURSORINFO();
        info.Size = Marshal.SizeOf(typeof(CURSORINFO));
        if (!GetCursorInfo(ref info))
        {
            throw new InvalidOperationException("MTGO_PREVIEW_CURSOR_QUERY_FAILED:" + Marshal.GetLastWin32Error());
        }
        bool showing = (info.Flags & CURSOR_SHOWING) != 0;
        bool inside = showing && info.ScreenPosition.X >= crop.Left && info.ScreenPosition.X < crop.Right &&
                      info.ScreenPosition.Y >= crop.Top && info.ScreenPosition.Y < crop.Bottom;
        return new MtgoCursorRecordV1
        {
            IsShowing = showing,
            X = info.ScreenPosition.X,
            Y = info.ScreenPosition.Y,
            IsInsideCrop = inside
        };
    }

    private static MtgoSignedRectV1 NewRect(int left, int top, int right, int bottom)
    {
        return new MtgoSignedRectV1 { Left = left, Top = top, Right = right, Bottom = bottom };
    }
}
'@
}

function Get-HexSha256FromBytes {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)
    $algorithm = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($algorithm.ComputeHash($Bytes))).Replace('-', '')
    }
    finally {
        $algorithm.Dispose()
    }
}

function Convert-RectToRecord {
    param([Parameter(Mandatory = $true)]$Rect)
    return [ordered]@{
        left = [int]$Rect.Left
        top = [int]$Rect.Top
        right = [int]$Rect.Right
        bottom = [int]$Rect.Bottom
        width = [long]$Rect.Width
        height = [long]$Rect.Height
    }
}

function Get-CheckedOutputPath {
    param([Parameter(Mandatory = $true)][string]$RequestedPath)

    if (-not [IO.Path]::IsPathRooted($RequestedPath)) {
        throw 'MTGO_PREVIEW_OUTPUT_PATH_MUST_BE_ABSOLUTE'
    }
    $requestedFullPath = [IO.Path]::GetFullPath($RequestedPath)
    if (Test-Path -LiteralPath $requestedFullPath) {
        throw 'MTGO_PREVIEW_OUTPUT_ALREADY_EXISTS'
    }
    $requestedParent = Split-Path -Parent $requestedFullPath
    $requestedLeaf = Split-Path -Leaf $requestedFullPath
    if ([string]::IsNullOrWhiteSpace($requestedLeaf)) {
        throw 'MTGO_PREVIEW_OUTPUT_LEAF_IS_EMPTY'
    }
    $resolvedParent = (Resolve-Path -LiteralPath $requestedParent -ErrorAction Stop).ProviderPath
    $resolvedOutput = Join-Path $resolvedParent $requestedLeaf

    $repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
    $repositoryPrefix = $repositoryRoot.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($resolvedOutput.Equals($repositoryRoot, [StringComparison]::OrdinalIgnoreCase) -or
        $resolvedOutput.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'MTGO_PREVIEW_OUTPUT_INSIDE_REPOSITORY_FORBIDDEN'
    }
    return $resolvedOutput
}

function Get-MtgoPreviewSnapshot {
    $processes = @(Get-Process -Name 'MTGO' -ErrorAction SilentlyContinue)
    if ($processes.Count -ne 1) {
        throw "MTGO_PREVIEW_EXPECTED_EXACTLY_ONE_PROCESS:$($processes.Count)"
    }

    $mtgoProcess = $processes[0]
    if (-not $mtgoProcess.Responding) {
        throw 'MTGO_PREVIEW_PROCESS_NOT_RESPONDING'
    }
    $executablePath = $mtgoProcess.Path
    if ([string]::IsNullOrWhiteSpace($executablePath)) {
        throw 'MTGO_PREVIEW_EXECUTABLE_PATH_UNAVAILABLE'
    }

    $file = Get-Item -LiteralPath $executablePath -ErrorAction Stop
    $productVersion = $file.VersionInfo.ProductVersion
    $fileVersion = $file.VersionInfo.FileVersion
    if ($productVersion -cne $ExpectedProductVersion -or $fileVersion -cne $ExpectedProductVersion) {
        throw "MTGO_PREVIEW_VERSION_MISMATCH:product=$productVersion,file=$fileVersion"
    }

    $executableSha256 = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToUpperInvariant()
    if ($executableSha256 -cne $ExpectedExecutableSha256.ToUpperInvariant()) {
        throw 'MTGO_PREVIEW_EXECUTABLE_HASH_MISMATCH'
    }

    $signature = Get-AuthenticodeSignature -LiteralPath $executablePath
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or $null -eq $signature.SignerCertificate) {
        throw "MTGO_PREVIEW_SIGNATURE_NOT_VALID:$($signature.Status)"
    }
    $signerThumbprint = $signature.SignerCertificate.Thumbprint.ToUpperInvariant()
    if ($signerThumbprint -cne $ExpectedSignerThumbprint.ToUpperInvariant()) {
        throw 'MTGO_PREVIEW_SIGNER_THUMBPRINT_MISMATCH'
    }
    if ($signature.SignerCertificate.Subject -cne $ExpectedSignerSubject) {
        throw 'MTGO_PREVIEW_SIGNER_SUBJECT_MISMATCH'
    }

    $mtgoProcessId = [uint32]$mtgoProcess.Id
    $windows = @([MtgoVisiblePreviewNativeV1]::VisibleTopLevelWindowsForProcess($mtgoProcessId))
    if ($windows.Count -eq 0 -or $windows.Count -gt 8) {
        throw "MTGO_PREVIEW_VISIBLE_TOP_LEVEL_WINDOW_COUNT_OUT_OF_RANGE:$($windows.Count)"
    }

    $mainClientWindows = @($windows | Where-Object {
        [MtgoVisiblePreviewNativeV1]::WindowTitle($_) -ceq $ExpectedWindowTitle
    })
    if ($mainClientWindows.Count -ne 1) {
        throw "MTGO_PREVIEW_EXPECTED_EXACTLY_ONE_MAIN_CLIENT_WINDOW:$($mainClientWindows.Count)"
    }
    $visibleWindowSetLines = @($windows | Sort-Object { $_.ToInt64() } | ForEach-Object {
        $candidateTitle = [MtgoVisiblePreviewNativeV1]::WindowTitle($_)
        '{0}:{1}:{2}' -f $_.ToInt64(), $candidateTitle.Length, $candidateTitle
    })
    $visibleWindowSetBytes = [Text.Encoding]::UTF8.GetBytes(
        "mtgo_visible_top_level_window_set_v1`n$($visibleWindowSetLines -join "`n")")
    $visibleWindowSetSha256 = Get-HexSha256FromBytes -Bytes $visibleWindowSetBytes

    $expectedWindowTitleRule = $ExpectedWindowTitle
    $captureRole = 'main_client'
    $gameWindowTitleIdentity = 'not_applicable'
    $baseGameWindowTitleRule = $null
    $identifiedGameWindowTitleRule = $null
    if ($TargetWindowMode -ceq 'MainClient') {
        if ($windows.Count -ne 1) {
            throw "MTGO_PREVIEW_EXPECTED_EXACTLY_ONE_VISIBLE_TOP_LEVEL_WINDOW:$($windows.Count)"
        }
        [IntPtr]$windowHandle = $mainClientWindows[0]
    }
    else {
        if ([string]::IsNullOrWhiteSpace($ExpectedGameFormat)) {
            throw 'MTGO_PREVIEW_EXPECTED_GAME_FORMAT_REQUIRED'
        }
        if ($windows.Count -lt 2) {
            throw "MTGO_PREVIEW_GAME_REQUIRES_MAIN_AND_DUEL_WINDOWS:$($windows.Count)"
        }
        [IntPtr]$windowHandle = [MtgoVisiblePreviewNativeV1]::GetForegroundWindow()
        if ($windowHandle -eq [IntPtr]::Zero -or
            [MtgoVisiblePreviewNativeV1]::ProcessIdForWindow($windowHandle) -ne $mtgoProcessId) {
            throw 'MTGO_PREVIEW_FOREGROUND_GAME_NOT_OWNED_BY_MTGO'
        }
        if ($windowHandle -eq [IntPtr]$mainClientWindows[0]) {
            throw 'MTGO_PREVIEW_FOREGROUND_GAME_IS_MAIN_CLIENT'
        }
        if (-not ($windows -contains $windowHandle)) {
            throw 'MTGO_PREVIEW_FOREGROUND_GAME_NOT_IN_VISIBLE_WINDOW_SET'
        }
        $escapedGameFormat = [Regex]::Escape($ExpectedGameFormat)
        if ($TargetWindowMode -ceq 'ForegroundSpectatorGame') {
            $captureRole = 'spectator'
            $baseGameWindowTitleRule = ('^\(1-on-1\): {0}: Vs\. [^,\r\n]+,\s*[^,#\r\n]+$' -f $escapedGameFormat)
            $identifiedGameWindowTitleRule = ('^\(1-on-1\): {0}: Vs\. [^,\r\n]+,\s*[^\r\n]+?\s+Match #\s*\d+\s*-\s*Game #\s*\d+$' -f $escapedGameFormat)
        }
        else {
            $captureRole = 'acting_player_solitaire'
            $baseGameWindowTitleRule = ('^\(Solitaire\): {0}: Vs\. [^,#\r\n]+$' -f $escapedGameFormat)
            $identifiedGameWindowTitleRule = ('^\(Solitaire\): {0}: Vs\. [^\r\n]+?\s+Match #\s*\d+\s*-\s*Game #\s*\d+$' -f $escapedGameFormat)
        }
        $expectedWindowTitleRule = "$identifiedGameWindowTitleRule OR $baseGameWindowTitleRule"
    }

    if (-not [MtgoVisiblePreviewNativeV1]::IsWindow($windowHandle) -or
        -not [MtgoVisiblePreviewNativeV1]::IsRootWindow($windowHandle)) {
        throw 'MTGO_PREVIEW_TARGET_IS_NOT_A_LIVE_ROOT_WINDOW'
    }
    if ([MtgoVisiblePreviewNativeV1]::ProcessIdForWindow($windowHandle) -ne $mtgoProcessId) {
        throw 'MTGO_PREVIEW_WINDOW_PROCESS_MISMATCH'
    }
    $windowTitle = [MtgoVisiblePreviewNativeV1]::WindowTitle($windowHandle)
    if ($TargetWindowMode -ceq 'MainClient') {
        if ($windowTitle -cne $ExpectedWindowTitle) {
            throw "MTGO_PREVIEW_WINDOW_TITLE_MISMATCH:$windowTitle"
        }
    }
    else {
        if ($windowTitle -cmatch $identifiedGameWindowTitleRule) {
            $gameWindowTitleIdentity = 'participants_and_match_game_ids'
        }
        elseif ($windowTitle -cmatch $baseGameWindowTitleRule) {
            $gameWindowTitleIdentity = 'participants_only'
        }
        else {
            throw "MTGO_PREVIEW_GAME_WINDOW_TITLE_MISMATCH:$windowTitle"
        }
    }
    if ([MtgoVisiblePreviewNativeV1]::GetForegroundWindow() -ne $windowHandle) {
        throw 'MTGO_PREVIEW_WINDOW_NOT_FOREGROUND'
    }
    if (-not [MtgoVisiblePreviewNativeV1]::IsWindowVisible($windowHandle)) {
        throw 'MTGO_PREVIEW_WINDOW_NOT_VISIBLE'
    }
    if ([MtgoVisiblePreviewNativeV1]::IsIconic($windowHandle)) {
        throw 'MTGO_PREVIEW_WINDOW_MINIMIZED'
    }
    if ([MtgoVisiblePreviewNativeV1]::CloakedState($windowHandle) -ne 0) {
        throw 'MTGO_PREVIEW_WINDOW_CLOAKED'
    }
    [uint32]$displayAffinity = 0
    if (-not [MtgoVisiblePreviewNativeV1]::GetWindowDisplayAffinity($windowHandle, [ref]$displayAffinity)) {
        throw 'MTGO_PREVIEW_DISPLAY_AFFINITY_QUERY_FAILED'
    }
    if ($displayAffinity -ne 0) {
        throw "MTGO_PREVIEW_DISPLAY_AFFINITY_FORBIDS_CAPTURE:$displayAffinity"
    }
    [bool]$compositionEnabled = $false
    $compositionResult = [MtgoVisiblePreviewNativeV1]::DwmIsCompositionEnabled([ref]$compositionEnabled)
    if ($compositionResult -lt 0 -or -not $compositionEnabled) {
        throw "MTGO_PREVIEW_DESKTOP_COMPOSITION_UNAVAILABLE:$compositionResult"
    }

    $dpi = [MtgoVisiblePreviewNativeV1]::GetDpiForWindow($windowHandle)
    if ($dpi -ne $ExpectedDpi) {
        throw "MTGO_PREVIEW_DPI_MISMATCH:$dpi"
    }
    $clientBounds = [MtgoVisiblePreviewNativeV1]::ClientBounds($windowHandle)
    if ($clientBounds.Width -le 0 -or $clientBounds.Height -le 0 -or
        $clientBounds.Width -gt [int]::MaxValue -or $clientBounds.Height -gt [int]::MaxValue) {
        throw 'MTGO_PREVIEW_INVALID_CLIENT_BOUNDS'
    }
    $extendedFrameBounds = [MtgoVisiblePreviewNativeV1]::ExtendedFrameBounds($windowHandle)

    $intersectingMonitors = @([MtgoVisiblePreviewNativeV1]::Monitors() | Where-Object {
        [MtgoVisiblePreviewNativeV1]::Intersects($_.Bounds, $clientBounds)
    })
    if ($intersectingMonitors.Count -ne 1) {
        throw "MTGO_PREVIEW_CLIENT_SPANS_OR_MISSES_MONITORS:$($intersectingMonitors.Count)"
    }
    $monitor = $intersectingMonitors[0]
    if (-not [MtgoVisiblePreviewNativeV1]::Contains($monitor.Bounds, $clientBounds)) {
        throw 'MTGO_PREVIEW_CLIENT_PARTIALLY_OFFSCREEN'
    }

    $occlusion = [MtgoVisiblePreviewNativeV1]::AuditOcclusion($windowHandle, $clientBounds)
    if (-not $occlusion.TargetFound) {
        throw 'MTGO_PREVIEW_TARGET_NOT_FOUND_IN_Z_ORDER'
    }
    if ($occlusion.Intersections -ne 0) {
        throw "MTGO_PREVIEW_OCCLUDED_BY_WINDOW_ABOVE:$($occlusion.Intersections)"
    }
    $cursor = [MtgoVisiblePreviewNativeV1]::Cursor($clientBounds)
    if ($cursor.IsInsideCrop) {
        throw 'MTGO_PREVIEW_CURSOR_INSIDE_CLIENT'
    }

    return [pscustomobject]@{
        process_id = [int]$mtgoProcessId
        process_start_utc = $mtgoProcess.StartTime.ToUniversalTime().ToString('O')
        executable_path = $executablePath
        product_version = $productVersion
        file_version = $fileVersion
        executable_sha256 = $executableSha256
        signer_subject = $signature.SignerCertificate.Subject
        signer_thumbprint = $signerThumbprint
        target_window_mode = $TargetWindowMode
        capture_role = $captureRole
        expected_game_format = $ExpectedGameFormat
        expected_window_title_rule = $expectedWindowTitleRule
        game_window_title_identity = $gameWindowTitleIdentity
        visible_mtgo_top_level_window_count = [int]$windows.Count
        visible_mtgo_top_level_window_set_sha256 = $visibleWindowSetSha256
        window_handle = $windowHandle.ToInt64()
        window_title = $windowTitle
        foreground = $true
        visible = $true
        minimized = $false
        cloaked = $false
        display_affinity = [int]$displayAffinity
        desktop_composition_enabled = $true
        dpi = [int]$dpi
        client_bounds = $clientBounds
        extended_frame_bounds = $extendedFrameBounds
        monitor = $monitor
        windows_examined_above_target = [int]$occlusion.WindowsExaminedAboveTarget
        occluder_intersections = [int]$occlusion.Intersections
        cursor_showing = [bool]$cursor.IsShowing
        cursor_x = [int]$cursor.X
        cursor_y = [int]$cursor.Y
        cursor_inside_client = [bool]$cursor.IsInsideCrop
    }
}

function Assert-SnapshotsMatch {
    param(
        [Parameter(Mandatory = $true)]$Before,
        [Parameter(Mandatory = $true)]$After
    )
    $fields = @(
        'process_id', 'process_start_utc', 'executable_path', 'product_version', 'file_version',
        'executable_sha256', 'signer_subject', 'signer_thumbprint', 'window_handle', 'window_title',
        'target_window_mode', 'capture_role', 'expected_game_format', 'expected_window_title_rule',
        'game_window_title_identity',
        'visible_mtgo_top_level_window_count', 'visible_mtgo_top_level_window_set_sha256',
        'foreground', 'visible', 'minimized', 'cloaked', 'display_affinity',
        'desktop_composition_enabled', 'dpi', 'occluder_intersections', 'cursor_inside_client'
    )
    foreach ($field in $fields) {
        if ($Before.$field -cne $After.$field) {
            throw "MTGO_PREVIEW_STATE_CHANGED_DURING_CAPTURE:$field"
        }
    }
    foreach ($rectName in @('client_bounds', 'extended_frame_bounds')) {
        foreach ($edge in @('Left', 'Top', 'Right', 'Bottom')) {
            if ($Before.$rectName.$edge -ne $After.$rectName.$edge) {
                throw "MTGO_PREVIEW_GEOMETRY_CHANGED_DURING_CAPTURE:$rectName.$edge"
            }
        }
    }
    if ($Before.monitor.Handle -ne $After.monitor.Handle -or
        $Before.monitor.DeviceName -cne $After.monitor.DeviceName) {
        throw 'MTGO_PREVIEW_MONITOR_CHANGED_DURING_CAPTURE'
    }
    foreach ($edge in @('Left', 'Top', 'Right', 'Bottom')) {
        if ($Before.monitor.Bounds.$edge -ne $After.monitor.Bounds.$edge) {
            throw "MTGO_PREVIEW_MONITOR_BOUNDS_CHANGED_DURING_CAPTURE:$edge"
        }
    }
}

$finalOutputPath = Get-CheckedOutputPath -RequestedPath $OutputDirectory
$expectedExecutableSha256Normalized = $ExpectedExecutableSha256.ToUpperInvariant()
$expectedSignerThumbprintNormalized = $ExpectedSignerThumbprint.ToUpperInvariant()
$previousDpiContext = [IntPtr]::Zero
$bitmap = $null
$graphics = $null
$memoryStream = $null
$partialOutputPath = $null
$outputFinalized = $false

try {
    $previousDpiContext = [MtgoVisiblePreviewNativeV1]::EnterPerMonitorV2()
    $before = Get-MtgoPreviewSnapshot

    $flushResult = [MtgoVisiblePreviewNativeV1]::DwmFlush()
    if ($flushResult -lt 0) {
        throw "MTGO_PREVIEW_DWM_FLUSH_FAILED:$flushResult"
    }

    $width = [int]$before.client_bounds.Width
    $height = [int]$before.client_bounds.Height
    $bitmap = [System.Drawing.Bitmap]::new($width, $height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    # CopyFromScreen rejects combined raster-operation enum values even though
    # native BitBlt accepts CAPTUREBLT as a flag. SourceCopy is the supported
    # composed-screen operation; the separate Z-order audit rejects overlays.
    $copyOperation = [System.Drawing.CopyPixelOperation]::SourceCopy
    $graphics.CopyFromScreen(
        [int]$before.client_bounds.Left,
        [int]$before.client_bounds.Top,
        0,
        0,
        [System.Drawing.Size]::new($width, $height),
        $copyOperation)
    $graphics.Dispose()
    $graphics = $null

    $after = Get-MtgoPreviewSnapshot
    Assert-SnapshotsMatch -Before $before -After $after

    $memoryStream = [IO.MemoryStream]::new()
    $bitmap.Save($memoryStream, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngBytes = $memoryStream.ToArray()
    $pngSha256 = Get-HexSha256FromBytes -Bytes $pngBytes
    $capturedAtUtc = [DateTime]::UtcNow.ToString('O')

    $artifactKind = switch ($TargetWindowMode) {
        'ForegroundSpectatorGame' { 'mtgo_visible_spectator_gameplay_calibration_preview_v1' }
        'ForegroundSolitaireGame' { 'mtgo_visible_solitaire_gameplay_calibration_preview_v1' }
        default { 'mtgo_visible_desktop_calibration_preview_v1' }
    }
    $manifest = [ordered]@{
        schema_version = 1
        artifact_kind = $artifactKind
        status = 'pending_visual_review'
        captured_at_utc = $capturedAtUtc
        capture_backend = 'system_drawing_copy_from_composed_screen_v1'
        pixel_source = 'visible_desktop_client_crop_only'
        profile_id = $null
        safe_for_semantic_evidence = $false
        safe_for_ocr = $false
        safe_for_policy_scoring = $false
        safe_for_input = $false
        expected_identity = [ordered]@{
            product_version = $ExpectedProductVersion
            executable_sha256 = $expectedExecutableSha256Normalized
            signer_thumbprint = $expectedSignerThumbprintNormalized
            signer_subject = $ExpectedSignerSubject
            dpi = $ExpectedDpi
            window_title = $ExpectedWindowTitle
            target_window_mode = $TargetWindowMode
            capture_role = $before.capture_role
            expected_game_format = $ExpectedGameFormat
            expected_window_title_rule = $before.expected_window_title_rule
            game_window_title_identity = $before.game_window_title_identity
        }
        observed_identity = [ordered]@{
            process_id = $before.process_id
            process_start_utc = $before.process_start_utc
            product_version = $before.product_version
            file_version = $before.file_version
            executable_sha256 = $before.executable_sha256
            signer_thumbprint = $before.signer_thumbprint
            signer_subject = $before.signer_subject
        }
        window = [ordered]@{
            title = $before.window_title
            target_window_mode = $before.target_window_mode
            capture_role = $before.capture_role
            game_window_title_identity = $before.game_window_title_identity
            visible_mtgo_top_level_window_count = $before.visible_mtgo_top_level_window_count
            visible_mtgo_top_level_window_set_sha256 = $before.visible_mtgo_top_level_window_set_sha256
            foreground = $before.foreground
            visible = $before.visible
            minimized = $before.minimized
            cloaked = $before.cloaked
            display_affinity = $before.display_affinity
            desktop_composition_enabled = $before.desktop_composition_enabled
            dpi = $before.dpi
            client_bounds_desktop_px = Convert-RectToRecord -Rect $before.client_bounds
            extended_frame_bounds_desktop_px = Convert-RectToRecord -Rect $before.extended_frame_bounds
        }
        monitor = [ordered]@{
            device_name = $before.monitor.DeviceName
            is_primary = $before.monitor.IsPrimary
            bounds_desktop_px = Convert-RectToRecord -Rect $before.monitor.Bounds
            work_area_desktop_px = Convert-RectToRecord -Rect $before.monitor.WorkArea
        }
        occlusion_audit = [ordered]@{
            target_found = $true
            windows_examined_above_target = $before.windows_examined_above_target
            intersecting_windows_above_target = $before.occluder_intersections
            cursor_showing = $before.cursor_showing
            cursor_inside_client = $before.cursor_inside_client
        }
        frame = [ordered]@{
            file = 'frame.png'
            width = $width
            height = $height
            format = 'png'
            sha256 = $pngSha256
        }
        review_requirements = @(
            'Confirm the image contains only pixels visibly rendered in the MTGO client area.',
            'Confirm no notification, tooltip, dialog, cursor, or other application obscures the client.',
            'Identify the exact client version, DPI, client size, and stable layout anchors.',
            'Create a separate reviewed calibration profile before any OCR, scoring, or input.'
        )
    }
    $manifestJson = $manifest | ConvertTo-Json -Depth 12
    $utf8NoBom = [Text.UTF8Encoding]::new($false)
    $partialOutputPath = "$finalOutputPath.partial-$([Guid]::NewGuid().ToString('N'))"
    [IO.Directory]::CreateDirectory($partialOutputPath) | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $partialOutputPath 'frame.png'), $pngBytes)
    [IO.File]::WriteAllText((Join-Path $partialOutputPath 'manifest.json'), $manifestJson + [Environment]::NewLine, $utf8NoBom)
    [IO.Directory]::Move($partialOutputPath, $finalOutputPath)
    $outputFinalized = $true

    [pscustomobject]@{
        status = 'pending_visual_review'
        output_directory = $finalOutputPath
        frame_sha256 = $pngSha256
        frame_width = $width
        frame_height = $height
        safe_for_semantic_evidence = $false
        safe_for_ocr = $false
        safe_for_policy_scoring = $false
        safe_for_input = $false
    } | ConvertTo-Json -Depth 4
}
finally {
    if ($null -ne $graphics) { $graphics.Dispose() }
    if ($null -ne $bitmap) { $bitmap.Dispose() }
    if ($null -ne $memoryStream) { $memoryStream.Dispose() }
    if (-not $outputFinalized -and
        -not [string]::IsNullOrWhiteSpace($partialOutputPath) -and
        [IO.Directory]::Exists($partialOutputPath)) {
        $allowedPartialPrefix = "$finalOutputPath.partial-"
        if (-not $partialOutputPath.StartsWith($allowedPartialPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'MTGO_PREVIEW_PARTIAL_CLEANUP_PATH_REJECTED'
        }
        [IO.Directory]::Delete($partialOutputPath, $true)
    }
    if ($previousDpiContext -ne [IntPtr]::Zero) {
        [MtgoVisiblePreviewNativeV1]::RestoreDpiContext($previousDpiContext)
    }
}
