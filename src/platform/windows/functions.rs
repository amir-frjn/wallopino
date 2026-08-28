// This piece of code is inspired by LuminWallpaper library: https://github.com/jensroth-git/LuminWallpaper
// AI is used to generate documentation and comments for the code.

use windows::{
    Win32::{
        Foundation::*,
        Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        System::{
            StationsAndDesktops::{
                DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, GetUserObjectInformationW,
                OpenInputDesktop, UOI_NAME,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{Shell::PathFindFileNameW, WindowsAndMessaging::*},
    },
    core::{Error as WinErr, PCWSTR, PWSTR},
};

use crate::platform::windows::{
    core::{AttachWindow, DesktopHandle, FullscreenOcclusionData, Handle, MonitorInfo},
    procs::fullscreen_window_enum_proc,
};

/// Determines whether the Windows desktop is currently locked or on a secure screen.
///
/// This function detects lock screen states by checking two conditions in sequence:
/// 1. Whether the current session is on a secure desktop (login screen, UAC prompt, Ctrl+Alt+Delete).
/// 2. Whether the foreground window belongs to the Windows LockApp.exe process.
///
/// # Returns
/// Returns `true` if the desktop is locked or on a secure screen, `false` otherwise.
///
/// # Detection Logic
///
/// The function performs the following steps:
/// 1. **Secure Desktop Check**: Calls `is_secure_desktop` to detect if the current
///    desktop is a secure environment (e.g., login screen, UAC).
/// 2. **Foreground Window Check**: If not on a secure desktop, retrieves the foreground
///    window and checks its owning process.
/// 3. **Process Name Verification**: Queries the full process image name and extracts
///    the filename to compare against `"LockApp.exe"` (case-insensitive).
///
/// # Behavior
/// - Returns `true` immediately if on a secure desktop.
/// - Returns `false` if the foreground window cannot be queried or is invalid.
/// - Returns `false` if the process name cannot be retrieved.
/// - Returns `true` only if the foreground window's process is `LockApp.exe`.
///
/// # Platform Support
/// This function is Windows-specific and targets Windows 8 and later, where the
/// `LockApp.exe` process is used for the lock screen.
///
/// # Safety
/// This function contains `unsafe` blocks for Windows API calls. The caller should
/// ensure that:
/// - The Windows API is available and accessible.
/// - The process has appropriate permissions for querying process information.
///
/// # Example
/// ```
/// if is_desktop_locked() {
///     println!("System is locked or on a secure desktop");
///     // Pause wallpaper animations, reduce CPU usage, etc.
/// } else {
///     println!("Desktop is active and unlocked");
/// }
/// ```
///
/// # See Also
/// - `LockApp.exe` – Windows lock screen application (introduced in Windows 8).
pub fn is_desktop_locked() -> bool {
    if is_secure_desktop() {
        return true;
    }

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }

        let mut pid = 0_u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return false;
        }

        let proc = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => Handle(h),
            Err(_) => return false,
        };

        let mut path = [0_u16; 260];
        let mut len = path.len() as u32;

        if QueryFullProcessImageNameW(
            proc.0,
            PROCESS_NAME_FORMAT(0),
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
        .is_err()
        {
            return false;
        }

        let filename = PathFindFileNameW(PWSTR(path.as_mut_ptr()));
        let filename = filename.as_wide();

        String::from_utf16_lossy(filename).eq_ignore_ascii_case("LockApp.exe")
    }
}

/// Determines if a monitor is occluded by fullscreen windows beyond a specified threshold.
///
/// This function enumerates all top-level windows and calculates what fraction of the
/// specified monitor's area is covered by fullscreen windows. It returns `true` if the
/// occlusion fraction meets or exceeds the given threshold.
///
/// # Parameters
/// - `monitor`: The monitor to check for occlusion. Contains position and dimensions.
/// - `threshold`: The occlusion fraction threshold (0.0 to 1.0). For example, `0.5`
///   means at least 50% of the monitor must be occluded to return `true`.
/// - `global_variable`: A mutable reference to the platform state, passed through to
///   the enumeration callback for context.
///
/// # Returns
/// Returns `Ok(true)` if the monitor is occluded beyond the threshold, `Ok(false)`
/// if not, or a [`windows::core::Error`] if the window enumeration fails.
///
/// # How It Works
/// 1. Creates a `FullscreenOcclusionData` instance to collect occlusion rectangles.
/// 2. Packages the platform state and occlusion data for the callback.
/// 3. Enumerates all top-level windows using `EnumWindows`.
/// 4. The callback (`fullscreen_window_enum_proc`) identifies fullscreen windows
///    and adds their rectangles to the occlusion data.
/// 5. Computes the occlusion fraction by sampling points across the monitor at a
///    step size of 100 pixels.
/// 6. Compares the fraction against the threshold.
///
/// # Errors
/// Returns an error if `EnumWindows` fails to enumerate windows.
///
/// # Notes
/// - The occlusion calculation samples the monitor area at a 100-pixel step size
///   for performance reasons. This provides a reasonable approximation.
/// - Only windows that are fullscreen on the target monitor are considered for occlusion.
/// - Windows that are cloaked or invisible are filtered out by the callback.
/// - This function is useful for pausing wallpaper animations when a fullscreen
///   application (like a game or video player) is covering the desktop.
///
/// # Platform Support
/// This function is Windows-specific and relies on the `EnumWindows` API.
///
/// # Example
/// ```
/// # use windows::core::Error;
/// # fn example() -> Result<(), Error> {
/// let mut platform = AttachWindow::initialize()?;
/// let monitor = platform.get_wallpaper_target(Some(0))?;
///
/// // Check if monitor is at least 70% occluded
/// if is_monitor_occluded(&monitor, 0.7, &mut platform)? {
///     println!("Monitor is heavily occluded - pausing animations");
/// } else {
///     println!("Monitor is visible - animations can run");
/// }
/// # Ok(())
/// # }
/// ```
///
pub fn is_monitor_occluded(
    monitor: &MonitorInfo,
    threshold: f64,
    global_variable: &mut AttachWindow,
) -> Result<bool, WinErr> {
    let mut occlusion_data = FullscreenOcclusionData::default();
    occlusion_data.monitor = monitor.clone();

    let mut data = (global_variable, &mut occlusion_data);
    let lparam = LPARAM(&mut data as *mut _ as isize);

    unsafe { EnumWindows(Some(fullscreen_window_enum_proc), lparam)? };
    let occlusion_fraction =
        compute_occlusion_fraction(&occlusion_data.occluded_rects, monitor, 100);
    Ok(occlusion_fraction >= threshold)
}

pub fn show_alert(title: &str, message: &str) {
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// Computes the fraction of a monitor's area covered by occluding rectangles.
///
/// This helper function samples points across the monitor's bounds at regular intervals
/// and determines what proportion of those points fall within any of the provided
/// occluding rectangles. The result is a value between `0.0` (no occlusion) and `1.0`
/// (fully occluded).
///
/// # Parameters
/// - `occluded_rects`: A slice of `RECT` structures representing occluded regions
///   (typically fullscreen windows or other overlay elements).
/// - `monitor`: The monitor information containing bounds to check.
/// - `sample_step`: The step size (in pixels) between sample points. Smaller values
///   give more accurate results but are computationally more expensive.
///
/// # Returns
/// Returns a `f64` between `0.0` and `1.0` representing the fraction of the monitor
/// that is occluded. Returns `0.0` if no sample points are checked (e.g., monitor
/// has zero area).
///
/// # Algorithm
/// 1. Iterates over the monitor's bounds using the specified step size.
/// 2. For each sample point `(x, y)`, checks if it falls within any occluding rectangle.
/// 3. Counts the total number of sample points and the number that are occluded.
/// 4. Returns the ratio: `occluded_count / total_samples`.
///
/// # Performance Considerations
/// - **Step size** affects performance: `sample_step = 50` checks ~2,073 samples
///   for a 1920x1080 monitor, while `sample_step = 100` checks ~518 samples.
/// - The function uses `any()` which short-circuits on the first match, improving
///   performance when early hits are common.
/// - For real-time applications, a step size of `100` is recommended as a good
///   balance between accuracy and speed.
///
/// # Examples
/// ```
/// # use windows::Win32::Foundation::RECT;
/// # use crate::platform::windows::models::MonitorInfo;
/// // Single rectangle covering the entire monitor
/// let rects = vec![
///     RECT { left: 0, top: 0, right: 1920, bottom: 1080 }
/// ];
/// let monitor = MonitorInfo {
///     x: 0,
///     y: 0,
///     width: 1920,
///     height: 1080,
///     ..Default::default()
/// };
///
/// let fraction = compute_occlusion_fraction(&rects, &monitor, 100);
/// assert_eq!(fraction, 1.0); // Fully occluded
/// ```
///
/// # Accuracy
/// - The accuracy of the result depends on the `sample_step` value and the
///   geometry of the occluding rectangles.
/// - Small rectangles may be missed if they don't align with sample points.
/// - For precise occlusion detection, use smaller step sizes or use direct
///   rectangle area calculations.
///
/// # See Also
/// - [`is_monitor_occluded`] for the public API that uses this function.
/// - [`FullscreenOcclusionData`] for the data structure containing occlusion rectangles.
fn compute_occlusion_fraction(
    occluded_rects: &[RECT],
    monitor: &MonitorInfo,
    sample_step: usize,
) -> f64 {
    let mut occluded_count = 0;
    let mut total_samples = 0;

    for y in (monitor.y..monitor.y + monitor.height).step_by(sample_step) {
        for x in (monitor.x..monitor.x + monitor.width).step_by(sample_step) {
            total_samples += 1;

            let is_occluded = occluded_rects
                .iter()
                .any(|rect| x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom);

            if is_occluded {
                occluded_count += 1;
            }
        }
    }
    if total_samples == 0 {
        return 0.0;
    }
    return occluded_count as f64 / total_samples as f64;
}

/// Determines if a window is cloaked (invisible) on Windows 10/11.
///
/// Cloaked windows are Windows 10/11 UWP (Universal Windows Platform) and AppX
/// application windows that are not visible to the user but still exist in the
/// window hierarchy. These windows are typically background apps, suspended apps,
/// or hidden system components.
///
/// # Parameters
/// - `hwnd`: The window handle to check for cloaking.
///
/// # Returns
/// Returns `true` if the window is cloaked (invisible), `false` otherwise.
///
/// # How It Works
/// 1. Queries the Desktop Window Manager (DWM) for the `DWMWA_CLOAKED` attribute.
/// 2. A non-zero value indicates the window is cloaked (hidden).
/// 3. Returns `false` if the DWM query fails or the window is not cloaked.
///
/// # Notes
/// - Cloaked windows should typically be **ignored** in window enumeration operations
///   as they are not visible to the user and don't affect the desktop appearance.
/// - This is particularly important for filtering out Windows 10/11 background apps
///   when enumerating windows for occlusion detection or wallpaper management.
/// - The `DWMWA_CLOAKED` attribute was introduced in Windows 8 and is fully supported
///   on Windows 10 and Windows 11.
/// - Cloaked windows can still receive messages and have a presence in the window
///   hierarchy, so filtering them is essential for accurate enumeration.
///
/// # Platform Support
/// This function requires Windows 8 or later (Windows DWM with cloaking support).
///
/// # Example
/// ```
/// # use windows::Win32::Foundation::HWND;
/// # use windows::core::Error;
/// #
/// # unsafe fn example() -> Result<(), Error> {
/// // In a window enumeration callback
/// let callback = |hwnd: HWND, _: LPARAM| -> BOOL {
///     if is_invisible_win10_background_app_window(hwnd) {
///         return TRUE; // Skip this window
///     }
///     
///     // Process visible windows only
///     // ...
///     # TRUE
/// };
///
/// EnumWindows(Some(callback), LPARAM(0))?;
/// # Ok(())
/// # }
/// ```
///
/// # See Also
/// - [DWMWA_CLOAKED documentation](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
/// - [`is_desktop_locked`] for detecting lock screen states
/// - [`is_monitor_occluded`] for occlusion detection that may need to filter cloaked windows
pub fn is_invisible_win10_background_app_window(hwnd: HWND) -> bool {
    let mut cloaked_val = 0;
    let hres = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked_val as *mut _ as *mut _,
            size_of_val(&cloaked_val) as u32,
        )
    };
    hres.is_ok() && cloaked_val != 0
}

/// Checks if the current desktop is a secure desktop (lock screen, UAC, login, etc.).
///
/// Secure desktops are protected environments used by Windows for sensitive operations.
/// This function returns `true` for any desktop that is not named `"Default"`.
///
/// # Returns
/// - `true`: Currently on a secure desktop.
/// - `false`: Currently on the default desktop.
///
/// # Conservative Behavior
/// Any failure during detection returns `true` to be safe.
///
/// # Example
/// ```no_run
/// if is_secure_desktop() {
///     // Pause UI updates on lock screen
/// }
/// ```
fn is_secure_desktop() -> bool {
    unsafe {
        let desktop = match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) {
            Ok(handle) => DesktopHandle(handle),
            Err(_) => return true, // couldn’t query ⇒ assume we’re secure to be conservative
        };

        let mut bytes = 0_u32;

        let result =
            GetUserObjectInformationW(HANDLE(desktop.0.0), UOI_NAME, None, 0, Some(&mut bytes));

        if result.is_err() && GetLastError() != ERROR_INSUFFICIENT_BUFFER {
            return true; // genuine failure
        }

        if bytes == 0 {
            return true; // shouldn’t happen, but stay conservative
        }

        // Allocate a UTF-16 buffer.
        //
        // Windows reports the required size in BYTES.
        let wchar_count = (bytes as usize / std::mem::size_of::<u16>()) + 1;

        let mut name = vec![0u16; wchar_count];

        let mut bytes_written = bytes;

        if GetUserObjectInformationW(
            HANDLE(desktop.0.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            bytes_written,
            Some(&mut bytes_written),
        )
        .is_err()
        {
            return true;
        }

        let name = String::from_utf16_lossy(&name);

        // Equivalent to:
        // _wcsicmp(name.data(), L"Default") != 0
        !name.trim_end_matches('\0').eq_ignore_ascii_case("Default")
    }
}

/// Checks if a `WorkerW` window exists between two windows in the Z-order.
///
/// This function traverses the Z-order upward from a target window (typically the
/// engine window) and checks if a `WorkerW` window is present before reaching
/// the `SHELLDLL_DefView` (desktop icons) window.
///
/// # Parameters
/// - `shell_def_view`: The handle to the `SHELLDLL_DefView` window (desktop icons).
/// - `target`: The handle to the target window to start checking from (typically the engine window).
///
/// # Returns
/// Returns `true` if a `WorkerW` window is found between the target and `SHELLDLL_DefView`,
/// `false` otherwise.
///
/// # How It Works
/// 1. Starts from the `target` window and walks up the Z-order using `GetWindow` with `GW_HWNDPREV`.
/// 2. Checks each window's class name.
/// 3. If a `WorkerW` window is encountered, returns `true`.
/// 4. If `SHELLDLL_DefView` is reached without finding a `WorkerW`, returns `false`.
///
/// # Why This Is Needed
/// On Windows, the system may dynamically replace `WorkerW` windows between SHELLDLL_DefView ans the engine
/// on events like changing wallpapers or switching desktops.
/// Detecting this allows the engine to reparent itself correctly to maintain proper layering.
///
///
/// # Example
/// ```
/// let has_workerw = has_workerw_between(shell_def_view, engine_hwnd);
/// if has_workerw {
///     // A WorkerW exists between the engine and desktop icons,
///     // so the engine needs to be reparented.
/// }
/// ```
pub fn has_workerw_between(shell_def_view: isize, target: isize) -> bool {
    let shell_def_view = HWND(shell_def_view as _);
    let target = HWND(target as _);
    let mut current = target;

    unsafe {
        loop {
            let previous = match GetWindow(current, GW_HWNDPREV) {
                Ok(hwnd) => hwnd,
                Err(_) => return false,
            };
            // We reached SHELLDLL_DefView without seeing WorkerW.
            if previous == shell_def_view {
                return false;
            }

            let (class_name, _) = class_and_title(previous);

            // A WokrerW found, we need to reparent the engine to it.
            if class_name == "WorkerW" {
                return true;
            }

            current = previous;
        }
    }
}

/// Retrieves the class name and window title for a given window handle.
///
/// # Arguments
/// * `hwnd` - Handle to the target window
///
/// # Returns
/// A tuple containing (class_name, window_title) as Strings.
/// Both strings are lossily decoded from UTF-16; invalid sequences are replaced with U+FFFD.
pub fn class_and_title(hwnd: HWND) -> (String, String) {
    let mut buffer = [0_u16; 512];

    // Get class name
    let class_len = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    let class_name = String::from_utf16_lossy(&buffer[..class_len]);

    // Get window title
    let title_len = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    let title = String::from_utf16_lossy(&buffer[..title_len]);

    (class_name, title)
}

/// Converts an integer window handle into a Windows [`HWND`].
///
/// This helper centralizes the conversion from the internally stored `isize`
/// representation to the Windows API handle type.
///
/// # Arguments
///
/// * `hwnd` - Integer representation of a Windows window handle.
///
/// # Returns
///
/// Returns the corresponding [`HWND`].
#[inline]
pub fn create_hwnd(hwnd: isize) -> HWND {
    HWND(hwnd as _)
}
