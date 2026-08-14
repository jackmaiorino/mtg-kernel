[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(0, 16383)]
    [int]$ClientX,

    [Parameter(Mandatory = $true)]
    [ValidateRange(0, 16383)]
    [int]$ClientY,

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, [int]::MaxValue)]
    [int]$ExpectedProcessId,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$ExpectedProcessStartUtc,

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
    [AllowEmptyString()]
    [string]$ExpectedWindowTitle,

    [ValidateSet('MainClient', 'ForegroundOwnedWindow', 'VisibleOwnedPopup')]
    [string]$TargetWindowMode = 'MainClient',

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 16384)]
    [int]$ExpectedClientWidth,

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 16384)]
    [int]$ExpectedClientHeight,

    [Parameter(Mandatory = $true)]
    [ValidateRange(96, 480)]
    [int]$ExpectedDpi,

    [ValidateRange(-32768, 32767)]
    [int]$ParkCursorX = 2500,

    [ValidateRange(-32768, 32767)]
    [int]$ParkCursorY = 1400
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrEmpty($ExpectedWindowTitle) -and
    $TargetWindowMode -cne 'ForegroundOwnedWindow' -and
    $TargetWindowMode -cne 'VisibleOwnedPopup') {
    throw 'MTGO_CLICK_EMPTY_TITLE_REQUIRES_OWNED_WINDOW'
}

if (-not ('MtgoSupervisedClickNativeV1' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class MtgoSupervisedClickNativeV1
{
    private const uint GA_ROOT = 2;
    private const uint GW_OWNER = 4;
    private const int DWMWA_CLOAKED = 14;
    private const int INPUT_MOUSE = 0;
    private const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    private const uint MOUSEEVENTF_LEFTUP = 0x0004;

    private delegate bool EnumWindowsProc(IntPtr window, IntPtr parameter);

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT
    {
        public int X;
        public int Y;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MOUSEINPUT
    {
        public int Dx;
        public int Dy;
        public uint MouseData;
        public uint DwFlags;
        public uint Time;
        public UIntPtr DwExtraInfo;
    }

    [StructLayout(LayoutKind.Explicit)]
    public struct INPUTUNION
    {
        [FieldOffset(0)] public MOUSEINPUT Mouse;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT
    {
        public int Type;
        public INPUTUNION Union;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);

    [DllImport("user32.dll")]
    private static extern IntPtr GetThreadDpiAwarenessContext();

    [DllImport("user32.dll")]
    private static extern bool AreDpiAwarenessContextsEqual(IntPtr first, IntPtr second);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern bool IsWindow(IntPtr window);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr window);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll")]
    public static extern bool IsIconic(IntPtr window);

    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr window);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool GetClientRect(IntPtr window, out RECT rect);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool ClientToScreen(IntPtr window, ref POINT point);

    [DllImport("user32.dll")]
    public static extern IntPtr WindowFromPoint(POINT point);

    [DllImport("user32.dll")]
    public static extern IntPtr GetAncestor(IntPtr window, uint flags);

    [DllImport("user32.dll")]
    private static extern IntPtr GetWindow(IntPtr window, uint command);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowTextW(IntPtr window, char[] text, int count);

    [DllImport("user32.dll")]
    private static extern int GetWindowTextLengthW(IntPtr window);

    [DllImport("user32.dll")]
    private static extern bool AttachThreadInput(uint attach, uint attachTo, bool value);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool GetCursorPos(out POINT point);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, INPUT[] inputs, int size);

    public static IntPtr EnterPerMonitorV2()
    {
        IntPtr target = new IntPtr(-4);
        IntPtr previous = SetThreadDpiAwarenessContext(target);
        if (previous == IntPtr.Zero || !AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), target))
        {
            throw new InvalidOperationException("MTGO_CLICK_DPI_CONTEXT_NOT_PER_MONITOR_V2");
        }
        return previous;
    }

    public static void RestoreDpiContext(IntPtr previous)
    {
        if (previous != IntPtr.Zero)
        {
            SetThreadDpiAwarenessContext(previous);
        }
    }

    public static bool ActivateWindow(IntPtr window)
    {
        IntPtr foreground = GetForegroundWindow();
        if (foreground == window)
        {
            return true;
        }

        uint ignored;
        uint foregroundThread = GetWindowThreadProcessId(foreground, out ignored);
        uint currentThread = GetCurrentThreadId();
        bool attached = foregroundThread != 0 && foregroundThread != currentThread &&
            AttachThreadInput(currentThread, foregroundThread, true);
        try
        {
            return SetForegroundWindow(window);
        }
        finally
        {
            if (attached)
            {
                AttachThreadInput(currentThread, foregroundThread, false);
            }
        }
    }

    public static bool IsUncloaked(IntPtr window)
    {
        int cloaked;
        return DwmGetWindowAttribute(window, DWMWA_CLOAKED, out cloaked, sizeof(int)) == 0 && cloaked == 0;
    }

    public static bool IsSameRootWindowAtPoint(IntPtr expectedRoot, POINT point)
    {
        IntPtr hit = WindowFromPoint(point);
        return hit != IntPtr.Zero && GetAncestor(hit, GA_ROOT) == expectedRoot;
    }

    public static bool IsRootWindow(IntPtr window)
    {
        return window != IntPtr.Zero && GetAncestor(window, GA_ROOT) == window;
    }

    public static IntPtr[] VisibleOwnedRootWindowsForProcess(IntPtr mainWindow, uint processId)
    {
        List<IntPtr> windows = new List<IntPtr>();
        EnumWindows(delegate(IntPtr candidate, IntPtr ignored)
        {
            uint candidateProcessId;
            GetWindowThreadProcessId(candidate, out candidateProcessId);
            if (candidate != mainWindow &&
                candidateProcessId == processId &&
                IsWindowVisible(candidate) &&
                IsRootWindow(candidate) &&
                GetWindow(candidate, GW_OWNER) == mainWindow)
            {
                windows.Add(candidate);
            }
            return true;
        }, IntPtr.Zero);
        windows.Sort(delegate(IntPtr left, IntPtr right)
        {
            return left.ToInt64().CompareTo(right.ToInt64());
        });
        return windows.ToArray();
    }

    public static string WindowTitle(IntPtr window)
    {
        int length = GetWindowTextLengthW(window);
        char[] buffer = new char[length + 1];
        int copied = GetWindowTextW(window, buffer, buffer.Length);
        return copied <= 0 ? String.Empty : new string(buffer, 0, copied);
    }

    public static void SendOneLeftClick()
    {
        INPUT[] inputs = new INPUT[2];
        inputs[0].Type = INPUT_MOUSE;
        inputs[0].Union.Mouse.DwFlags = MOUSEEVENTF_LEFTDOWN;
        inputs[1].Type = INPUT_MOUSE;
        inputs[1].Union.Mouse.DwFlags = MOUSEEVENTF_LEFTUP;
        uint sent = SendInput(2, inputs, Marshal.SizeOf(typeof(INPUT)));
        if (sent != 2)
        {
            throw new InvalidOperationException("MTGO_CLICK_SEND_INPUT_FAILED");
        }
    }
}
'@
}

$previousDpiContext = [IntPtr]::Zero
try {
    $previousDpiContext = [MtgoSupervisedClickNativeV1]::EnterPerMonitorV2()

    $processes = @(Get-Process -Name MTGO -ErrorAction Stop)
    if ($processes.Count -ne 1) {
        throw "MTGO_CLICK_PROCESS_COUNT_MISMATCH:$($processes.Count)"
    }

    $process = $processes[0]
    if ($process.Id -ne $ExpectedProcessId) {
        throw 'MTGO_CLICK_PROCESS_ID_MISMATCH'
    }
    $actualStartUtc = $process.StartTime.ToUniversalTime().ToString('o')
    $expectedStart = [DateTimeOffset]::Parse($ExpectedProcessStartUtc).UtcDateTime
    if ($process.StartTime.ToUniversalTime() -ne $expectedStart) {
        throw 'MTGO_CLICK_PROCESS_START_MISMATCH'
    }
    if (-not $process.Responding) {
        throw 'MTGO_CLICK_PROCESS_NOT_RESPONDING'
    }

    $actualExecutableSha256 = (Get-FileHash -LiteralPath $process.Path -Algorithm SHA256).Hash
    if ($actualExecutableSha256 -cne $ExpectedExecutableSha256.ToUpperInvariant()) {
        throw 'MTGO_CLICK_EXECUTABLE_HASH_MISMATCH'
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $process.Path
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
        $null -eq $signature.SignerCertificate) {
        throw "MTGO_CLICK_SIGNATURE_NOT_VALID:$($signature.Status)"
    }
    if ($signature.SignerCertificate.Thumbprint.ToUpperInvariant() -cne
        $ExpectedSignerThumbprint.ToUpperInvariant()) {
        throw 'MTGO_CLICK_SIGNER_THUMBPRINT_MISMATCH'
    }
    if ($signature.SignerCertificate.Subject -cne $ExpectedSignerSubject) {
        throw 'MTGO_CLICK_SIGNER_SUBJECT_MISMATCH'
    }

    [IntPtr]$mainWindow = $process.MainWindowHandle
    if ($TargetWindowMode -ceq 'MainClient') {
        [IntPtr]$window = $mainWindow
    }
    elseif ($TargetWindowMode -ceq 'ForegroundOwnedWindow') {
        [IntPtr]$window = [MtgoSupervisedClickNativeV1]::GetForegroundWindow()
    }
    else {
        if ([MtgoSupervisedClickNativeV1]::GetForegroundWindow() -ne $mainWindow) {
            throw 'MTGO_CLICK_OWNED_POPUP_REQUIRES_FOREGROUND_MAIN_CLIENT'
        }
        $ownedPopups = @([MtgoSupervisedClickNativeV1]::VisibleOwnedRootWindowsForProcess(
            $mainWindow,
            [uint32]$ExpectedProcessId))
        if ($ownedPopups.Count -ne 1) {
            throw "MTGO_CLICK_VISIBLE_OWNED_POPUP_COUNT_MISMATCH:$($ownedPopups.Count)"
        }
        [IntPtr]$window = $ownedPopups[0]
    }
    if (-not [MtgoSupervisedClickNativeV1]::IsWindow($window) -or
        -not [MtgoSupervisedClickNativeV1]::IsRootWindow($window) -or
        [MtgoSupervisedClickNativeV1]::WindowTitle($window) -cne $ExpectedWindowTitle) {
        throw 'MTGO_CLICK_WINDOW_IDENTITY_MISMATCH'
    }
    [uint32]$windowProcessId = 0
    [void][MtgoSupervisedClickNativeV1]::GetWindowThreadProcessId($window, [ref]$windowProcessId)
    if ($windowProcessId -ne $ExpectedProcessId) {
        throw 'MTGO_CLICK_WINDOW_PROCESS_MISMATCH'
    }
    if (-not [MtgoSupervisedClickNativeV1]::IsWindowVisible($window) -or
        [MtgoSupervisedClickNativeV1]::IsIconic($window) -or
        -not [MtgoSupervisedClickNativeV1]::IsUncloaked($window)) {
        throw 'MTGO_CLICK_WINDOW_NOT_VISIBLE_READY'
    }
    if ([MtgoSupervisedClickNativeV1]::GetDpiForWindow($window) -ne $ExpectedDpi) {
        throw 'MTGO_CLICK_WINDOW_DPI_MISMATCH'
    }

    $clientRect = New-Object MtgoSupervisedClickNativeV1+RECT
    if (-not [MtgoSupervisedClickNativeV1]::GetClientRect($window, [ref]$clientRect)) {
        throw 'MTGO_CLICK_CLIENT_RECT_FAILED'
    }
    $clientWidth = $clientRect.Right - $clientRect.Left
    $clientHeight = $clientRect.Bottom - $clientRect.Top
    if ($clientWidth -ne $ExpectedClientWidth -or $clientHeight -ne $ExpectedClientHeight) {
        throw "MTGO_CLICK_CLIENT_SIZE_MISMATCH:${clientWidth}x${clientHeight}"
    }
    if ($ClientX -ge $clientWidth -or $ClientY -ge $clientHeight) {
        throw 'MTGO_CLICK_POINT_OUTSIDE_CLIENT'
    }

    if ($TargetWindowMode -ceq 'VisibleOwnedPopup') {
        if ([MtgoSupervisedClickNativeV1]::GetForegroundWindow() -ne $mainWindow) {
            throw 'MTGO_CLICK_OWNED_POPUP_FOREGROUND_CHANGED'
        }
    }
    else {
        if (-not [MtgoSupervisedClickNativeV1]::ActivateWindow($window)) {
            throw 'MTGO_CLICK_ACTIVATION_FAILED'
        }
        Start-Sleep -Milliseconds 200
        if ([MtgoSupervisedClickNativeV1]::GetForegroundWindow() -ne $window) {
            throw 'MTGO_CLICK_FOREGROUND_MISMATCH'
        }
    }

    $screenPoint = New-Object MtgoSupervisedClickNativeV1+POINT
    $screenPoint.X = $ClientX
    $screenPoint.Y = $ClientY
    if (-not [MtgoSupervisedClickNativeV1]::ClientToScreen($window, [ref]$screenPoint)) {
        throw 'MTGO_CLICK_CLIENT_TO_SCREEN_FAILED'
    }
    if (-not [MtgoSupervisedClickNativeV1]::IsSameRootWindowAtPoint($window, $screenPoint)) {
        throw 'MTGO_CLICK_VISIBLE_HIT_TARGET_MISMATCH'
    }
    if (-not [MtgoSupervisedClickNativeV1]::SetCursorPos($screenPoint.X, $screenPoint.Y)) {
        throw 'MTGO_CLICK_SET_CURSOR_FAILED'
    }
    Start-Sleep -Milliseconds 100
    $actualCursor = New-Object MtgoSupervisedClickNativeV1+POINT
    if (-not [MtgoSupervisedClickNativeV1]::GetCursorPos([ref]$actualCursor) -or
        $actualCursor.X -ne $screenPoint.X -or $actualCursor.Y -ne $screenPoint.Y) {
        throw 'MTGO_CLICK_CURSOR_POSITION_CHANGED'
    }
    $expectedForeground = if ($TargetWindowMode -ceq 'VisibleOwnedPopup') {
        $mainWindow
    }
    else {
        $window
    }
    if ([MtgoSupervisedClickNativeV1]::GetForegroundWindow() -ne $expectedForeground -or
        -not [MtgoSupervisedClickNativeV1]::IsSameRootWindowAtPoint($window, $screenPoint)) {
        throw 'MTGO_CLICK_PRE_INPUT_STATE_CHANGED'
    }

    [MtgoSupervisedClickNativeV1]::SendOneLeftClick()
    Start-Sleep -Milliseconds 100
    [void][MtgoSupervisedClickNativeV1]::SetCursorPos($ParkCursorX, $ParkCursorY)

    [ordered]@{
        schema_version = 1
        action = 'one_supervised_visible_left_click'
        process_id = $ExpectedProcessId
        process_start_utc = $actualStartUtc
        window_handle = $window.ToInt64()
        window_title = $ExpectedWindowTitle
        target_window_mode = $TargetWindowMode
        dpi = $ExpectedDpi
        client_size_px = [ordered]@{ width = $clientWidth; height = $clientHeight }
        client_point_px = [ordered]@{ x = $ClientX; y = $ClientY }
        screen_point_px = [ordered]@{ x = $screenPoint.X; y = $screenPoint.Y }
        postcondition_verified = $false
        safe_for_autonomous_input = $false
        safe_for_purchase = $false
        safe_for_queue_entry = $false
    } | ConvertTo-Json -Depth 4
}
finally {
    if ($previousDpiContext -ne [IntPtr]::Zero) {
        [MtgoSupervisedClickNativeV1]::RestoreDpiContext($previousDpiContext)
    }
}
