#[cfg(not(target_os = "windows"))]
compile_error!("mtgo-dxgi-capture-v1 is Windows-only");

use mtgo_dxgi_capture_v1::{copy_tightly_packed_bgra8_v1, sha256_hex_v1, SignedRectV1};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, FILETIME, HANDLE, HMODULE, HWND, POINT, RECT,
};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dwm::{
    DwmFlush, DwmGetWindowAttribute, DwmIsCompositionEnabled, DWMWA_CLOAKED,
    DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_FORMAT_B8G8R8A8_UNORM,
    DXGI_MODE_ROTATION_IDENTITY, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIOutput6,
    IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_NOT_FOUND, DXGI_OUTDUPL_FRAME_INFO,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Security::Cryptography::{
    CertCloseStore, CertFindCertificateInStore, CertFreeCertificateContext,
    CertGetCertificateContextProperty, CertNameToStrW, CryptMsgClose, CryptMsgGetParam,
    CryptQueryObject, CERT_CONTEXT, CERT_FIND_SUBJECT_CERT, CERT_HASH_PROP_ID, CERT_INFO,
    CERT_NAME_STR_REVERSE_FLAG, CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
    CERT_QUERY_FORMAT_FLAG_BINARY, CERT_QUERY_OBJECT_FILE, CERT_STRING_TYPE, CERT_X500_NAME_STR,
    CMSG_SIGNER_INFO, CMSG_SIGNER_INFO_PARAM, HCERTSTORE, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};
use windows::Win32::Security::WinTrust::{
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UICONTEXT_EXECUTE, WTD_UI_NONE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetDpiForWindow, GetThreadDpiAwarenessContext,
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClientRect, GetCursorInfo, GetForegroundWindow, GetTopWindow, GetWindow,
    GetWindowDisplayAffinity, GetWindowTextW, GetWindowThreadProcessId, IsHungAppWindow, IsIconic,
    IsWindow, IsWindowVisible, CURSORINFO, CURSOR_SHOWING, GA_ROOT, GW_HWNDNEXT, WDA_NONE,
};

type ProbeResult<T> = Result<T, String>;

#[derive(Debug)]
struct CliV1 {
    output_directory: PathBuf,
    expected_executable_sha256: String,
    expected_signer_thumbprint: String,
    expected_signer_subject_sha256: String,
    expected_title_contains: String,
    timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WindowSnapshotV1 {
    hwnd: u64,
    process_id: u32,
    mtgo_process_count: u32,
    process_start_filetime_100ns: u64,
    process_image: String,
    executable_sha256: String,
    authenticode_valid: bool,
    signer_thumbprint: String,
    signer_subject: String,
    signer_subject_sha256: String,
    title: String,
    dpi: u32,
    client_rect_desktop_px: SignedRectV1,
    extended_frame_rect_desktop_px: SignedRectV1,
    foreground: bool,
    visible: bool,
    minimized: bool,
    cloaked: bool,
    hung: bool,
    display_affinity: u32,
    desktop_composition_enabled: bool,
    cursor_showing: bool,
    cursor_x: i32,
    cursor_y: i32,
    cursor_inside_client: bool,
    occlusion_target_found: bool,
    occluding_windows_above: u32,
    z_order_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct OutputIdentityV1 {
    adapter_index: u32,
    output_index: u32,
    adapter_luid_low: u32,
    adapter_luid_high: i32,
    device_name: String,
    bounds_desktop_px: SignedRectV1,
    rotation: i32,
    color_space: i32,
}

#[derive(Debug, Serialize)]
struct FrameMetadataV1 {
    last_present_time_qpc: i64,
    last_mouse_update_time_qpc: i64,
    accumulated_frames: u32,
    protected_content_masked_out: bool,
    pointer_visible: bool,
    pointer_x: i32,
    pointer_y: i32,
    source_texture_width: u32,
    source_texture_height: u32,
    source_texture_format: i32,
    canonical_width: u32,
    canonical_height: u32,
    canonical_stride: u32,
    canonical_byte_length: usize,
    canonical_bgra8_sha256: String,
    preview_png_sha256: String,
}

#[derive(Debug, Serialize)]
struct CaptureManifestV1 {
    schema: &'static str,
    artifact_kind: &'static str,
    status: &'static str,
    capture_backend: &'static str,
    captured_at_unix_millis: u128,
    safety: SafetyFlagsV1,
    pre: WindowSnapshotV1,
    post: WindowSnapshotV1,
    output: OutputIdentityV1,
    frame: FrameMetadataV1,
    files: FilesV1,
}

#[derive(Debug, Serialize)]
struct SafetyFlagsV1 {
    safe_for_semantic_evidence: bool,
    safe_for_ocr: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
    authenticode_verified_in_probe: bool,
}

#[derive(Debug, Serialize)]
struct FilesV1 {
    canonical_pixels: &'static str,
    preview_png: &'static str,
    manifest: &'static str,
}

struct CapturedFrameV1 {
    pixels: Vec<u8>,
    preview_png: Vec<u8>,
    metadata: FrameMetadataV1,
    output: OutputIdentityV1,
}

struct ProcessHandleV1(windows::Win32::Foundation::HANDLE);

struct CryptQueryGuardV1 {
    store: HCERTSTORE,
    message: *const c_void,
}

impl Drop for CryptQueryGuardV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = CryptMsgClose(Some(self.message));
            let _ = CertCloseStore(Some(self.store), 0);
        }
    }
}

struct CertificateGuardV1(*const CERT_CONTEXT);

impl Drop for CertificateGuardV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = CertFreeCertificateContext(Some(self.0));
        }
    }
}

struct AuthenticodeIdentityV1 {
    thumbprint: String,
    subject: String,
    subject_sha256: String,
}

impl Drop for ProcessHandleV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct DpiContextGuardV1(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);

impl Drop for DpiContextGuardV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = SetThreadDpiAwarenessContext(self.0);
        }
    }
}

struct DuplicationFrameGuardV1<'a>(&'a IDXGIOutputDuplication);

impl Drop for DuplicationFrameGuardV1<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.ReleaseFrame();
        }
    }
}

struct MappedTextureGuardV1<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}

impl Drop for MappedTextureGuardV1<'_> {
    fn drop(&mut self) {
        unsafe {
            self.context.Unmap(self.texture, 0);
        }
    }
}

fn main() {
    match run() {
        Ok(output) => {
            println!("{}", output.display());
        }
        Err(error) => {
            eprintln!("MTGO_DXGI_CAPTURE_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}

fn run() -> ProbeResult<PathBuf> {
    let cli = parse_cli()?;
    validate_sha256(&cli.expected_executable_sha256)?;
    validate_sha1(&cli.expected_signer_thumbprint)?;
    validate_sha256(&cli.expected_signer_subject_sha256)?;
    let output_directory = validate_output_destination(&cli.output_directory)?;
    let _dpi_guard = enter_per_monitor_v2()?;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err("foreground window is null".to_owned());
    }
    let pre = snapshot_window(hwnd, &cli)?;
    require_admitted_window(&pre)?;

    unsafe { DwmFlush().map_err(|error| format!("DwmFlush before capture: {error}"))? };
    let captured = capture_dxgi_frame(pre.client_rect_desktop_px, cli.timeout_ms)?;
    let post = snapshot_window(hwnd, &cli)?;
    require_admitted_window(&post)?;
    if pre != post {
        return Err(
            "window, process, focus, geometry, cursor, or z-order changed during capture"
                .to_owned(),
        );
    }
    if captured.metadata.pointer_visible
        && pre
            .client_rect_desktop_px
            .contains_point(captured.metadata.pointer_x, captured.metadata.pointer_y)
    {
        return Err("DXGI pointer position intersects the client crop".to_owned());
    }

    let manifest = CaptureManifestV1 {
        schema: "mtgo-dxgi-visible-frame-candidate/v1",
        artifact_kind: "mtgo_untrusted_dxgi_visible_frame_candidate_v1",
        status: "checked_untrusted_not_admitted",
        capture_backend: "dxgi_desktop_duplication_v1",
        captured_at_unix_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before epoch: {error}"))?
            .as_millis(),
        safety: SafetyFlagsV1 {
            safe_for_semantic_evidence: false,
            safe_for_ocr: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
            authenticode_verified_in_probe: true,
        },
        pre,
        post,
        output: captured.output,
        frame: captured.metadata,
        files: FilesV1 {
            canonical_pixels: "frame.bgra",
            preview_png: "frame.png",
            manifest: "manifest.json",
        },
    };
    persist_atomically(
        &output_directory,
        &captured.pixels,
        &captured.preview_png,
        &manifest,
    )?;
    Ok(output_directory)
}

fn parse_cli() -> ProbeResult<CliV1> {
    let mut args = std::env::args().skip(1);
    let mut output_directory = None;
    let mut expected_executable_sha256 = None;
    let mut expected_signer_thumbprint = None;
    let mut expected_signer_subject_sha256 = None;
    let mut expected_title_contains = "Magic: The Gathering Online".to_owned();
    let mut timeout_ms = 1_500u32;
    while let Some(argument) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {argument}"))?;
        match argument.as_str() {
            "--output" => output_directory = Some(PathBuf::from(value)),
            "--expected-exe-sha256" => expected_executable_sha256 = Some(value),
            "--expected-signer-thumbprint" => expected_signer_thumbprint = Some(value),
            "--expected-signer-subject-sha256" => expected_signer_subject_sha256 = Some(value),
            "--expected-title-contains" => expected_title_contains = value,
            "--timeout-ms" => {
                timeout_ms = value
                    .parse::<u32>()
                    .map_err(|_| "timeout must be an integer".to_owned())?;
                if !(100..=10_000).contains(&timeout_ms) {
                    return Err("timeout must be between 100 and 10000 milliseconds".to_owned());
                }
            }
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    Ok(CliV1 {
        output_directory: output_directory.ok_or("--output is required")?,
        expected_executable_sha256: expected_executable_sha256
            .ok_or("--expected-exe-sha256 is required")?,
        expected_signer_thumbprint: expected_signer_thumbprint
            .ok_or("--expected-signer-thumbprint is required")?,
        expected_signer_subject_sha256: expected_signer_subject_sha256
            .ok_or("--expected-signer-subject-sha256 is required")?,
        expected_title_contains,
        timeout_ms,
    })
}

fn enter_per_monitor_v2() -> ProbeResult<DpiContextGuardV1> {
    unsafe {
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if previous.0.is_null() {
            return Err("SetThreadDpiAwarenessContext failed".to_owned());
        }
        if !AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
        .as_bool()
        {
            let _ = SetThreadDpiAwarenessContext(previous);
            return Err("capture thread is not Per-Monitor V2 aware".to_owned());
        }
        Ok(DpiContextGuardV1(previous))
    }
}

fn snapshot_window(hwnd: HWND, cli: &CliV1) -> ProbeResult<WindowSnapshotV1> {
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() || GetAncestor(hwnd, GA_ROOT) != hwnd {
            return Err("foreground target is not a root window".to_owned());
        }
        let mut process_id = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&mut process_id)) == 0 || process_id == 0 {
            return Err("could not resolve foreground process".to_owned());
        }
        let handle = ProcessHandleV1(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
                .map_err(|error| format!("OpenProcess: {error}"))?,
        );
        let mtgo_process_count = count_mtgo_processes(process_id)?;
        let process_image = query_process_image(handle.0)?;
        if !Path::new(&process_image)
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("MTGO.exe"))
        {
            return Err("foreground process image is not MTGO.exe".to_owned());
        }
        let process_image_path = Path::new(&process_image);
        let executable_sha256 = sha256_file(process_image_path)?;
        if executable_sha256 != cli.expected_executable_sha256 {
            return Err(
                "foreground MTGO executable hash does not match the expected hash".to_owned(),
            );
        }
        let signer = verify_authenticode(process_image_path)?;
        if signer.thumbprint != cli.expected_signer_thumbprint
            || signer.subject_sha256 != cli.expected_signer_subject_sha256
        {
            return Err("MTGO signer identity does not match the pinned certificate".to_owned());
        }
        let title = window_title(hwnd);
        if cli.expected_title_contains.is_empty() || !title.contains(&cli.expected_title_contains) {
            return Err(
                "foreground title does not match the configured MTGO title rule".to_owned(),
            );
        }
        let client_rect = client_rect_desktop(hwnd)?;
        let extended_rect = dwm_rect(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS)?;
        if !extended_rect.contains(client_rect) {
            return Err("DWM frame bounds do not contain the client crop".to_owned());
        }
        let cloaked = dwm_u32(hwnd, DWMWA_CLOAKED)? != 0;
        let mut display_affinity = u32::MAX;
        GetWindowDisplayAffinity(hwnd, &mut display_affinity)
            .map_err(|error| format!("GetWindowDisplayAffinity: {error}"))?;
        let composition = DwmIsCompositionEnabled()
            .map_err(|error| format!("DwmIsCompositionEnabled: {error}"))?
            .as_bool();
        let cursor = cursor_state(client_rect)?;
        let occlusion = audit_occlusion(hwnd, client_rect)?;
        Ok(WindowSnapshotV1 {
            hwnd: hwnd.0 as usize as u64,
            process_id,
            mtgo_process_count,
            process_start_filetime_100ns: process_start_filetime(handle.0)?,
            process_image,
            executable_sha256,
            authenticode_valid: true,
            signer_thumbprint: signer.thumbprint,
            signer_subject: signer.subject,
            signer_subject_sha256: signer.subject_sha256,
            title,
            dpi: GetDpiForWindow(hwnd),
            client_rect_desktop_px: client_rect,
            extended_frame_rect_desktop_px: extended_rect,
            foreground: GetForegroundWindow() == hwnd,
            visible: IsWindowVisible(hwnd).as_bool(),
            minimized: IsIconic(hwnd).as_bool(),
            cloaked,
            hung: IsHungAppWindow(hwnd).as_bool(),
            display_affinity,
            desktop_composition_enabled: composition,
            cursor_showing: cursor.0,
            cursor_x: cursor.1,
            cursor_y: cursor.2,
            cursor_inside_client: cursor.3,
            occlusion_target_found: occlusion.0,
            occluding_windows_above: occlusion.1,
            z_order_sha256: occlusion.2,
        })
    }
}

fn require_admitted_window(snapshot: &WindowSnapshotV1) -> ProbeResult<()> {
    if !snapshot.foreground
        || !snapshot.visible
        || snapshot.minimized
        || snapshot.cloaked
        || snapshot.hung
        || !snapshot.authenticode_valid
        || snapshot.mtgo_process_count != 1
        || snapshot.display_affinity != WDA_NONE.0
        || !snapshot.desktop_composition_enabled
        || snapshot.dpi == 0
        || snapshot.cursor_inside_client
        || !snapshot.occlusion_target_found
        || snapshot.occluding_windows_above != 0
    {
        return Err("window admission checks did not all pass".to_owned());
    }
    Ok(())
}

fn capture_dxgi_frame(client_rect: SignedRectV1, timeout_ms: u32) -> ProbeResult<CapturedFrameV1> {
    unsafe {
        let factory: IDXGIFactory1 =
            CreateDXGIFactory1().map_err(|error| format!("CreateDXGIFactory1: {error}"))?;
        let mut adapter_index = 0u32;
        loop {
            let adapter = match factory.EnumAdapters1(adapter_index) {
                Ok(value) => value,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(format!("EnumAdapters1: {error}")),
            };
            let mut output_index = 0u32;
            loop {
                let output = match adapter.EnumOutputs(output_index) {
                    Ok(value) => value,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(format!("EnumOutputs: {error}")),
                };
                let desc = output
                    .GetDesc()
                    .map_err(|error| format!("IDXGIOutput::GetDesc: {error}"))?;
                let output_rect = rect(desc.DesktopCoordinates);
                if output_rect.contains(client_rect) {
                    return capture_from_output(
                        adapter,
                        output
                            .cast()
                            .map_err(|error| format!("IDXGIOutput1: {error}"))?,
                        output
                            .cast()
                            .map_err(|error| format!("IDXGIOutput6: {error}"))?,
                        adapter_index,
                        output_index,
                        client_rect,
                        output_rect,
                        timeout_ms,
                    );
                }
                output_index += 1;
            }
            adapter_index += 1;
        }
    }
    Err("client rectangle is not wholly contained in one DXGI output".to_owned())
}

#[allow(clippy::too_many_arguments)]
unsafe fn capture_from_output(
    adapter: IDXGIAdapter1,
    output1: IDXGIOutput1,
    output6: IDXGIOutput6,
    adapter_index: u32,
    output_index: u32,
    client_rect: SignedRectV1,
    output_rect: SignedRectV1,
    timeout_ms: u32,
) -> ProbeResult<CapturedFrameV1> {
    let output_desc = output1
        .GetDesc()
        .map_err(|error| format!("output GetDesc: {error}"))?;
    let output_desc1 = output6
        .GetDesc1()
        .map_err(|error| format!("output GetDesc1: {error}"))?;
    if !output_desc.AttachedToDesktop.as_bool()
        || output_desc.Rotation != DXGI_MODE_ROTATION_IDENTITY
        || output_desc1.ColorSpace != DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
    {
        return Err("output is detached, rotated, HDR, or not canonical SDR sRGB".to_owned());
    }

    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    D3D11CreateDevice(
        &adapter,
        D3D_DRIVER_TYPE_UNKNOWN,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        Some(&[D3D_FEATURE_LEVEL_11_0]),
        D3D11_SDK_VERSION,
        Some(&mut device),
        None,
        Some(&mut context),
    )
    .map_err(|error| format!("D3D11CreateDevice: {error}"))?;
    let device = device.ok_or("D3D11CreateDevice returned no device")?;
    let context = context.ok_or("D3D11CreateDevice returned no context")?;
    let duplication = output1
        .DuplicateOutput(&device)
        .map_err(|error| format!("DuplicateOutput: {error}"))?;
    let duplication_desc = duplication.GetDesc();
    if duplication_desc.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        || duplication_desc.Rotation != DXGI_MODE_ROTATION_IDENTITY
    {
        return Err("desktop duplication format or rotation is not canonical BGRA8".to_owned());
    }

    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_ms as u64))
        .ok_or("capture deadline overflow")?;
    let (info, desktop_resource) = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for a real desktop presentation".to_owned());
        }
        let wait_ms = u32::try_from(remaining.as_millis().max(1))
            .unwrap_or(u32::MAX)
            .min(timeout_ms);
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut desktop_resource: Option<IDXGIResource> = None;
        duplication
            .AcquireNextFrame(wait_ms, &mut info, &mut desktop_resource)
            .map_err(|error| format!("AcquireNextFrame: {error}"))?;
        if info.ProtectedContentMaskedOut.as_bool() {
            let _ = duplication.ReleaseFrame();
            return Err("DXGI frame masks protected content".to_owned());
        }
        if info.LastPresentTime != 0 && info.AccumulatedFrames != 0 {
            break (info, desktop_resource);
        }
        duplication
            .ReleaseFrame()
            .map_err(|error| format!("ReleaseFrame after pointer-only update: {error}"))?;
    };
    let _frame_guard = DuplicationFrameGuardV1(&duplication);
    let desktop_texture: ID3D11Texture2D = desktop_resource
        .ok_or("AcquireNextFrame returned no desktop resource")?
        .cast()
        .map_err(|error| format!("desktop resource is not Texture2D: {error}"))?;
    let mut source_desc = D3D11_TEXTURE2D_DESC::default();
    desktop_texture.GetDesc(&mut source_desc);
    if source_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        || source_desc.Width != output_rect.width().map_err(str::to_owned)?
        || source_desc.Height != output_rect.height().map_err(str::to_owned)?
    {
        return Err("DXGI source texture does not match the selected output".to_owned());
    }
    let crop = client_rect
        .crop_box_within(output_rect)
        .map_err(str::to_owned)?;
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: crop.width,
        Height: crop.height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging: Option<ID3D11Texture2D> = None;
    device
        .CreateTexture2D(&staging_desc, None, Some(&mut staging))
        .map_err(|error| format!("CreateTexture2D staging: {error}"))?;
    let staging = staging.ok_or("CreateTexture2D returned no staging texture")?;
    let source_box = D3D11_BOX {
        left: crop.left,
        top: crop.top,
        front: 0,
        right: crop.left + crop.width,
        bottom: crop.top + crop.height,
        back: 1,
    };
    context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &desktop_texture, 0, Some(&source_box));
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    context
        .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        .map_err(|error| format!("Map staging texture: {error}"))?;
    let _mapped_guard = MappedTextureGuardV1 {
        context: &context,
        texture: &staging,
    };
    let mapped_len = usize::try_from(mapped.RowPitch)
        .ok()
        .and_then(|row| row.checked_mul(crop.height as usize))
        .ok_or("mapped texture byte length overflow")?;
    let mapped_bytes = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), mapped_len);
    let pixels = copy_tightly_packed_bgra8_v1(
        mapped_bytes,
        mapped.RowPitch as usize,
        crop.width,
        crop.height,
    )
    .map_err(str::to_owned)?;
    let preview_png = encode_preview_png(&pixels, crop.width, crop.height)?;

    let adapter_desc = adapter
        .GetDesc1()
        .map_err(|error| format!("adapter GetDesc1: {error}"))?;
    let output = OutputIdentityV1 {
        adapter_index,
        output_index,
        adapter_luid_low: adapter_desc.AdapterLuid.LowPart,
        adapter_luid_high: adapter_desc.AdapterLuid.HighPart,
        device_name: utf16_nul(&output_desc.DeviceName),
        bounds_desktop_px: output_rect,
        rotation: output_desc.Rotation.0,
        color_space: output_desc1.ColorSpace.0,
    };
    let metadata = FrameMetadataV1 {
        last_present_time_qpc: info.LastPresentTime,
        last_mouse_update_time_qpc: info.LastMouseUpdateTime,
        accumulated_frames: info.AccumulatedFrames,
        protected_content_masked_out: info.ProtectedContentMaskedOut.as_bool(),
        pointer_visible: info.PointerPosition.Visible.as_bool(),
        pointer_x: info.PointerPosition.Position.x,
        pointer_y: info.PointerPosition.Position.y,
        source_texture_width: source_desc.Width,
        source_texture_height: source_desc.Height,
        source_texture_format: source_desc.Format.0,
        canonical_width: crop.width,
        canonical_height: crop.height,
        canonical_stride: crop.width.checked_mul(4).ok_or("stride overflow")?,
        canonical_byte_length: pixels.len(),
        canonical_bgra8_sha256: sha256_hex_v1(&pixels),
        preview_png_sha256: sha256_hex_v1(&preview_png),
    };
    Ok(CapturedFrameV1 {
        pixels,
        preview_png,
        metadata,
        output,
    })
}

fn client_rect_desktop(hwnd: HWND) -> ProbeResult<SignedRectV1> {
    unsafe {
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client).map_err(|error| format!("GetClientRect: {error}"))?;
        let mut upper_left = POINT {
            x: client.left,
            y: client.top,
        };
        let mut lower_right = POINT {
            x: client.right,
            y: client.bottom,
        };
        if !ClientToScreen(hwnd, &mut upper_left).as_bool()
            || !ClientToScreen(hwnd, &mut lower_right).as_bool()
        {
            return Err("ClientToScreen failed".to_owned());
        }
        let value = SignedRectV1 {
            left: upper_left.x,
            top: upper_left.y,
            right: lower_right.x,
            bottom: lower_right.y,
        };
        value.width().map_err(str::to_owned)?;
        value.height().map_err(str::to_owned)?;
        Ok(value)
    }
}

fn dwm_rect(
    hwnd: HWND,
    attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
) -> ProbeResult<SignedRectV1> {
    unsafe {
        let mut value = RECT::default();
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&mut value as *mut RECT).cast::<c_void>(),
            std::mem::size_of::<RECT>() as u32,
        )
        .map_err(|error| format!("DwmGetWindowAttribute rect: {error}"))?;
        let value = rect(value);
        value.width().map_err(str::to_owned)?;
        value.height().map_err(str::to_owned)?;
        Ok(value)
    }
}

fn dwm_u32(
    hwnd: HWND,
    attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
) -> ProbeResult<u32> {
    unsafe {
        let mut value = 0u32;
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&mut value as *mut u32).cast::<c_void>(),
            std::mem::size_of::<u32>() as u32,
        )
        .map_err(|error| format!("DwmGetWindowAttribute u32: {error}"))?;
        Ok(value)
    }
}

fn cursor_state(client: SignedRectV1) -> ProbeResult<(bool, i32, i32, bool)> {
    unsafe {
        let mut cursor = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        GetCursorInfo(&mut cursor).map_err(|error| format!("GetCursorInfo: {error}"))?;
        let showing = cursor.flags.0 & CURSOR_SHOWING.0 != 0;
        Ok((
            showing,
            cursor.ptScreenPos.x,
            cursor.ptScreenPos.y,
            showing && client.contains_point(cursor.ptScreenPos.x, cursor.ptScreenPos.y),
        ))
    }
}

fn audit_occlusion(hwnd: HWND, client: SignedRectV1) -> ProbeResult<(bool, u32, String)> {
    unsafe {
        let mut current = GetTopWindow(None).map_err(|error| format!("GetTopWindow: {error}"))?;
        let mut lines = Vec::new();
        let mut intersections = 0u32;
        for _ in 0..4_096 {
            if current == hwnd {
                lines.push(format!("target:{}", current.0 as usize));
                return Ok((
                    true,
                    intersections,
                    sha256_hex_v1(lines.join("\n").as_bytes()),
                ));
            }
            if IsWindowVisible(current).as_bool() && dwm_u32(current, DWMWA_CLOAKED)? == 0 {
                let bounds = dwm_rect(current, DWMWA_EXTENDED_FRAME_BOUNDS)?;
                let intersects = bounds.intersects(client);
                if intersects {
                    intersections = intersections
                        .checked_add(1)
                        .ok_or("occlusion count overflow")?;
                }
                lines.push(format!(
                    "{}:{}:{}:{}:{}:{}",
                    current.0 as usize,
                    bounds.left,
                    bounds.top,
                    bounds.right,
                    bounds.bottom,
                    intersects
                ));
            }
            current = GetWindow(current, GW_HWNDNEXT)
                .map_err(|error| format!("GetWindow(GW_HWNDNEXT) before target: {error}"))?;
        }
    }
    Err("target was not found within the top-level z-order audit bound".to_owned())
}

fn query_process_image(handle: windows::Win32::Foundation::HANDLE) -> ProbeResult<String> {
    unsafe {
        let mut buffer = vec![0u16; 32_768];
        let mut length = buffer.len() as u32;
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .map_err(|error| format!("QueryFullProcessImageNameW: {error}"))?;
        buffer.truncate(length as usize);
        String::from_utf16(&buffer).map_err(|error| format!("process path is not UTF-16: {error}"))
    }
}

fn count_mtgo_processes(target_process_id: u32) -> ProbeResult<u32> {
    let snapshot = ProcessHandleV1(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|error| format!("CreateToolhelp32Snapshot: {error}"))?,
    );
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Process32FirstW(snapshot.0, &mut entry) }
        .map_err(|error| format!("Process32FirstW: {error}"))?;
    let mut count = 0u32;
    let mut target_found = false;
    loop {
        if utf16_nul(&entry.szExeFile).eq_ignore_ascii_case("MTGO.exe") {
            count = count.checked_add(1).ok_or("MTGO process count overflow")?;
            target_found |= entry.th32ProcessID == target_process_id;
        }
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(format!("Process32NextW: {error}")),
        }
    }
    if !target_found {
        return Err("foreground MTGO process was absent from the process snapshot".to_owned());
    }
    Ok(count)
}

fn process_start_filetime(handle: windows::Win32::Foundation::HANDLE) -> ProbeResult<u64> {
    unsafe {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
            .map_err(|error| format!("GetProcessTimes: {error}"))?;
        Ok(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
    }
}

fn window_title(hwnd: HWND) -> String {
    unsafe {
        let mut buffer = vec![0u16; 2_048];
        let length = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
        buffer.truncate(length);
        String::from_utf16_lossy(&buffer)
    }
}

fn sha256_file(path: &Path) -> ProbeResult<String> {
    let mut file = File::open(path).map_err(|error| format!("open executable: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1_048_576];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("read executable: {error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verify_authenticode(path: &Path) -> ProbeResult<AuthenticodeIdentityV1> {
    let mut wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide_path.as_mut_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut trust_data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        )
    };
    trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
    let close_status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        )
    };
    if status != 0 {
        return Err(format!(
            "WinVerifyTrust rejected the MTGO executable with status {status:#010x}"
        ));
    }
    if close_status != 0 {
        return Err(format!(
            "WinVerifyTrust state close failed with status {close_status:#010x}"
        ));
    }
    signer_identity_from_file(&wide_path)
}

fn signer_identity_from_file(wide_path: &[u16]) -> ProbeResult<AuthenticodeIdentityV1> {
    let mut store = HCERTSTORE::default();
    let mut message: *mut c_void = std::ptr::null_mut();
    unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide_path.as_ptr().cast::<c_void>(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            None,
            None,
            None,
            Some(&mut store),
            Some(&mut message),
            None,
        )
        .map_err(|error| format!("CryptQueryObject signer extraction: {error}"))?;
    }
    if store.0.is_null() || message.is_null() {
        return Err("CryptQueryObject returned no signer store or message".to_owned());
    }
    let _query_guard = CryptQueryGuardV1 {
        store,
        message: message.cast_const(),
    };

    let mut signer_bytes = 0u32;
    unsafe {
        CryptMsgGetParam(
            message.cast_const(),
            CMSG_SIGNER_INFO_PARAM,
            0,
            None,
            &mut signer_bytes,
        )
        .map_err(|error| format!("CryptMsgGetParam signer size: {error}"))?;
    }
    let word_size = std::mem::size_of::<usize>();
    let words = (signer_bytes as usize)
        .checked_add(word_size - 1)
        .ok_or("signer allocation overflow")?
        / word_size;
    let mut signer_storage = vec![0usize; words];
    unsafe {
        CryptMsgGetParam(
            message.cast_const(),
            CMSG_SIGNER_INFO_PARAM,
            0,
            Some(signer_storage.as_mut_ptr().cast::<c_void>()),
            &mut signer_bytes,
        )
        .map_err(|error| format!("CryptMsgGetParam signer data: {error}"))?;
    }
    let signer = unsafe { &*signer_storage.as_ptr().cast::<CMSG_SIGNER_INFO>() };
    let cert_info = CERT_INFO {
        Issuer: signer.Issuer,
        SerialNumber: signer.SerialNumber,
        ..Default::default()
    };
    let encoding = X509_ASN_ENCODING | PKCS_7_ASN_ENCODING;
    let certificate = unsafe {
        CertFindCertificateInStore(
            store,
            encoding,
            0,
            CERT_FIND_SUBJECT_CERT,
            Some((&cert_info as *const CERT_INFO).cast::<c_void>()),
            None,
        )
    };
    if certificate.is_null() {
        return Err("embedded Authenticode signer certificate was not found".to_owned());
    }
    let _certificate_guard = CertificateGuardV1(certificate);

    let mut thumbprint_bytes = 0u32;
    unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_HASH_PROP_ID,
            None,
            &mut thumbprint_bytes,
        )
        .map_err(|error| format!("certificate thumbprint size: {error}"))?;
    }
    let mut thumbprint = vec![0u8; thumbprint_bytes as usize];
    unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_HASH_PROP_ID,
            Some(thumbprint.as_mut_ptr().cast::<c_void>()),
            &mut thumbprint_bytes,
        )
        .map_err(|error| format!("certificate thumbprint: {error}"))?;
    }
    thumbprint.truncate(thumbprint_bytes as usize);
    let thumbprint = thumbprint
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let cert_info = unsafe { &*(*certificate).pCertInfo };
    let string_type = CERT_STRING_TYPE(CERT_X500_NAME_STR.0 | CERT_NAME_STR_REVERSE_FLAG);
    let subject_len = unsafe { CertNameToStrW(encoding, &cert_info.Subject, string_type, None) };
    if subject_len <= 1 {
        return Err("certificate subject is empty".to_owned());
    }
    let mut subject_buffer = vec![0u16; subject_len as usize];
    let written = unsafe {
        CertNameToStrW(
            encoding,
            &cert_info.Subject,
            string_type,
            Some(&mut subject_buffer),
        )
    };
    if written != subject_len {
        return Err("certificate subject length changed during extraction".to_owned());
    }
    subject_buffer.truncate((written - 1) as usize);
    let subject = String::from_utf16(&subject_buffer)
        .map_err(|error| format!("certificate subject is not UTF-16: {error}"))?;
    let subject_sha256 = sha256_hex_v1(subject.as_bytes());
    Ok(AuthenticodeIdentityV1 {
        thumbprint,
        subject,
        subject_sha256,
    })
}

fn validate_output_destination(requested: &Path) -> ProbeResult<PathBuf> {
    if !requested.is_absolute() || requested.exists() {
        return Err("output must be a new absolute directory".to_owned());
    }
    let parent = requested
        .parent()
        .ok_or("output directory has no parent")?
        .canonicalize()
        .map_err(|error| format!("canonicalize output parent: {error}"))?;
    let name = requested
        .file_name()
        .ok_or("output directory has no name")?;
    let resolved = parent.join(name);
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not derive repository root")?
        .canonicalize()
        .map_err(|error| format!("canonicalize repository root: {error}"))?;
    if resolved.starts_with(repository) {
        return Err("capture output may not be written anywhere inside the repository".to_owned());
    }
    Ok(resolved)
}

fn persist_atomically(
    output: &Path,
    pixels: &[u8],
    preview_png: &[u8],
    manifest: &CaptureManifestV1,
) -> ProbeResult<()> {
    let parent = output.parent().ok_or("output has no parent")?;
    let partial = parent.join(format!(
        ".mtgo-dxgi-partial-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before epoch: {error}"))?
            .as_nanos()
    ));
    if partial.exists() {
        return Err("partial output path already exists".to_owned());
    }
    fs::create_dir(&partial).map_err(|error| format!("create partial directory: {error}"))?;
    let result = (|| {
        fs::write(partial.join("frame.bgra"), pixels)
            .map_err(|error| format!("write canonical pixels: {error}"))?;
        fs::write(partial.join("frame.png"), preview_png)
            .map_err(|error| format!("write preview PNG: {error}"))?;
        let manifest_bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|error| format!("serialize manifest: {error}"))?;
        fs::write(partial.join("manifest.json"), manifest_bytes)
            .map_err(|error| format!("write manifest: {error}"))?;
        fs::rename(&partial, output)
            .map_err(|error| format!("commit output directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() && partial.exists() {
        let _ = fs::remove_dir_all(&partial);
    }
    result
}

fn encode_preview_png(pixels: &[u8], width: u32, height: u32) -> ProbeResult<Vec<u8>> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(height as usize))
        .and_then(|value| value.checked_mul(4))
        .ok_or("PNG geometry overflow")?;
    if pixels.len() != expected {
        return Err("canonical BGRA length does not match PNG geometry".to_owned());
    }
    let mut rgba = Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("write PNG header: {error}"))?;
        writer
            .write_image_data(&rgba)
            .map_err(|error| format!("write PNG pixels: {error}"))?;
    }
    Ok(encoded)
}

fn validate_sha256(value: &str) -> ProbeResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected executable hash must be lowercase SHA-256".to_owned());
    }
    Ok(())
}

fn validate_sha1(value: &str) -> ProbeResult<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected signer thumbprint must be lowercase SHA-1".to_owned());
    }
    Ok(())
}

fn rect(value: RECT) -> SignedRectV1 {
    SignedRectV1 {
        left: value.left,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
    }
}

fn utf16_nul(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}
