# Local MTGO client inventory, 2026-08-08

This inventory used only installed-app registry values, ordinary file metadata, Authenticode validation, a file hash, and a normal process/window-title check. It did not launch MTGO, inspect assemblies or manifests, attach to a process, enumerate UI Automation, inspect memory or network traffic, or modify files.

## Observed installation

- Installed product: Magic The Gathering Online
- Installed version: `3.4.157.4686`
- Publisher: Daybreak Game Company LLC
- Packaging: per-user ClickOnce deployment
- Deployment architecture marker: `msil`
- Runtime and UI family: .NET WPF
- Process state during inventory: not running

Installed-app record:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\dfc99f0d036aa330
```

The uninstall registration invokes `dfshim.dll` against `MTGO.application` with public key token `e0b489d8605198df` and `processorArchitecture=msil`. The active executable was under the user's versioned `%LOCALAPPDATA%\Apps\2.0` ClickOnce cache.

Executable metadata observed for this installation:

```text
ProductName: Magic Online
FileDescription: MainNavigation
CompanyName: Wizards of the Coast
FileVersion: 3.4.157.4686
ProductVersion: 3.4.157.4686
Authenticode status: Valid
Signer: Daybreak Game Company LLC
SHA-256: 8B2D1F75367F3CE8C4A954A9A26D9489548C58A1C1F1B6DA04826F1659AE6F52
```

Normal filenames supporting the WPF classification included:

```text
Mtgo.Shared.Wpf.dll
Mtgo.Shared.Wpf.Mvvm.dll
Frames.Framework.Wpf.dll
Microsoft.Xaml.Behaviors.dll
System.Windows.Interactivity.dll
MTGO.exe.config
```

These observations identify packaging and UI technology. They are not an authorization to inspect managed internals, and no such inspection was performed.

## Adapter implications

- Do not hardcode the executable directory. ClickOnce cache paths change across updates.
- Discover a running top-level client from normal process and window metadata, then verify product version and Authenticode signer before observation. The recorded executable hash is a drift signal for this installation, not a permanent allowlist.
- Capture composed monitor pixels and crop to the visibly rendered client area. This is more conservative than a direct window-capture path that might return content while the client is occluded.
- Keep the client unobscured in a dedicated monitor or desktop region. Halt when it is minimized, covered, partially off-screen, resized unexpectedly, or moved across monitors with different scale factors.
- Record physical-pixel client bounds, monitor DPI, client version, layout identity, and frame SHA-256 with each decision. Never use fixed global coordinates.
- Perform OCR and card recognition only on captured visible pixels. Installed files, uncorroborated UI Automation properties, logs, manifests, process data, and network data must not become game-state inputs.
- Treat any client update or unrecognized visual layout as a calibration invalidation and stop.
- After any later authorized input, capture a new visible frame and require a semantic visible postcondition before allowing another input.

No live frame, layout, window size, or DPI behavior has been measured yet because MTGO was not running.
