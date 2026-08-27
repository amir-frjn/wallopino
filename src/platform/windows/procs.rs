// Some parts of module is inspired by LuminWallpaper library: https://github.com/jensroth-git/LuminWallpaper
// AI is used to generate documentation for the code.

use std::{cell::RefCell, sync::mpsc::Sender};

use windows::{
    Win32::{
        Foundation::{FALSE, HWND, LPARAM, LRESULT, RECT, TRUE, WPARAM},
        Graphics::Gdi::{GetMonitorInfoW, HDC, HMONITOR, IntersectRect, MONITORINFOEXW},
        UI::WindowsAndMessaging::{
            CallNextHookEx, FindWindowExW, GetShellWindow, GetWindowRect, HC_ACTION, HHOOK,
            IsIconic, IsWindowVisible, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED,
            MSLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
            WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
        },
    },
    core::{BOOL, w},
};

use crate::platform::windows::{
    core::{AttachWindow, FullscreenOcclusionData, MonitorInfo},
    functions::{class_and_title, is_invisible_win10_background_app_window},
    mouse::Events,
};

/// Enumerates visible top-level windows and records the portions that overlap
/// the target monitor.
///
/// This callback is used with `EnumWindows` to determine which windows are
/// currently covering the monitor occupied by the wallpaper or target window.
/// Windows that are irrelevant to occlusion calculations, such as the shell,
/// `WorkerW`, minimized windows, and known background application windows, are
/// ignored.
///
/// For every eligible window, its screen coordinates are converted into the
/// virtual desktop coordinate system used by [`AttachWindow`]. The resulting
/// intersection with the target monitor is stored in
/// [`FullscreenOcclusionData::occluded_rects`].
///
/// # Arguments
///
/// * `hwnd` - Handle of the top-level window currently being enumerated.
/// * `lparam` - Pointer to a tuple containing mutable references to
///   [`AttachWindow`] and [`FullscreenOcclusionData`].
///
/// # Filtering
///
/// The following windows are skipped:
///
/// - The engine window itself
/// - The `WorkerW` window used by the wallpaper system
/// - Invisible windows
/// - Minimized windows
/// - The Windows shell window
/// - Windows with the `WorkerW` class
/// - The `CEF-OSC-WIDGET` Nvidia overlay
/// - Known invisible Windows 10 background application windows
///
/// # Returns
///
/// Always returns `TRUE` so that `EnumWindows` continues enumerating the
/// remaining top-level windows.
///
/// # Safety
///
/// The `lparam` value must contain a valid pointer to the expected tuple of
/// mutable references for the lifetime of the enumeration callback.
pub extern "system" fn fullscreen_window_enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let wrapped = &mut *(lparam.0 as *mut (&mut AttachWindow, &mut FullscreenOcclusionData));
        let (g, occlusion_data) = wrapped;

        if Some(hwnd) == g.engine_window_handle || Some(hwnd) == g.workerw_window_handle {
            return TRUE;
        }
        // Skip non-visible or minimized windows.
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return TRUE;
        }

        //Make sure it isn't the shell window
        if GetShellWindow() == hwnd {
            return TRUE;
        }

        let sz_class_name = class_and_title(hwnd).0;
        // make sure it isnt a workerw window
        if sz_class_name == "WorkerW" {
            return TRUE;
        }

        // Check that it isn't the Nvidia overlay
        if sz_class_name == "CEF-OSC-WIDGET" {
            return TRUE;
        }

        // Skip the invisible windows that are part of the Windows 10 background app
        if is_invisible_win10_background_app_window(hwnd) {
            return TRUE;
        }

        let mut window_rect = RECT::default();

        if GetWindowRect(hwnd, &mut window_rect).is_ok() {
            // convert window rect to desktop coordinates
            window_rect.left -= g.desktop_x;
            window_rect.right -= g.desktop_x;
            window_rect.top -= g.desktop_y;
            window_rect.bottom -= g.desktop_y;

            // Build a rectangle for the target monitor.
            let monitor_rect = RECT {
                left: occlusion_data.monitor.x,
                top: occlusion_data.monitor.y,
                right: occlusion_data.monitor.x + occlusion_data.monitor.width,
                bottom: occlusion_data.monitor.y + occlusion_data.monitor.height,
            };

            // Calculate the intersection of the window's rectangle with the monitor's rectangle.
            let mut intersection_rect = RECT::default();

            if IntersectRect(&mut intersection_rect, &window_rect, &monitor_rect).as_bool() {
                // store the occluded area
                occlusion_data.occluded_rects.push(intersection_rect);
            }
        }
    }
    return TRUE;
}

/// Searches the top-level window hierarchy for the desktop's `WorkerW` window.
///
/// This callback is used with `EnumWindows` to locate a top-level window
/// containing a `SHELLDLL_DefView` child. Once such a window is found, the
/// callback searches for the corresponding `WorkerW` sibling and stores its
/// handle in [`AttachWindow::workerw_window_handle`].
///
/// The required [`AttachWindow`] instance is passed through `lparam` rather
/// than using global state.
///
/// # Arguments
///
/// * `window_handle` - Handle of the top-level window currently being examined.
/// * `lparam` - Pointer to a mutable [`AttachWindow`] used to store the
///   discovered `WorkerW` handle.
///
/// # Returns
///
/// Returns `FALSE` once the appropriate desktop window has been found, which
/// stops `EnumWindows` from continuing the enumeration.
///
/// Returns `TRUE` when the current window does not contain a
/// `SHELLDLL_DefView`, allowing enumeration to continue.
///
/// # Safety
///
/// The `lparam` value must point to a valid [`AttachWindow`] instance for the
/// duration of the enumeration.
pub extern "system" fn enum_windows_proc(window_handle: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        // Look for a child window named "SHELLDLL_DefView" in each top-level window.
        if let Ok(shell_view_window) =
            FindWindowExW(window_handle.into(), None, w!("SHELLDLL_DefView"), None)
        {
            if !shell_view_window.is_invalid() {
                // If found, get the WorkerW window that is a sibling of the found window.
                let g = { &mut *(lparam.0 as *mut AttachWindow) };
                g.workerw_window_handle =
                    { FindWindowExW(None, Some(window_handle), w!("WorkerW"), None) }.ok();
                return FALSE;
            }
        }
    }
    TRUE
}

/// Collects information about a display monitor during monitor enumeration.
///
/// This callback is used with `EnumDisplayMonitors` to retrieve the monitor's
/// full display area and its working area. The collected information is stored
/// as a [`MonitorInfo`] value in the vector supplied through `lparam`.
///
/// Both monitor and work-area coordinates are kept in the Windows virtual
/// desktop coordinate system, allowing the caller to correctly handle
/// multi-monitor layouts where monitors may have negative coordinates.
///
/// # Arguments
///
/// * `monitor_handle` - Handle identifying the monitor being enumerated.
/// * `_monitor_device_context` - Device context associated with the monitor.
/// * `_monitor_rectangle` - Optional monitor rectangle supplied by
///   `EnumDisplayMonitors`; it is not used because the callback retrieves the
///   authoritative rectangle through `GetMonitorInfoW`.
/// * `lparam` - Pointer to the `Vec<MonitorInfo>` that receives the collected
///   monitor information.
///
/// # Returns
///
/// Returns `TRUE` so that monitor enumeration continues.
///
/// # Safety
///
/// The `lparam` value must point to a valid mutable `Vec<MonitorInfo>` for the
/// duration of the enumeration.
pub extern "system" fn monitor_enum_proc(
    monitor_handle: HMONITOR,
    _monitor_device_context: HDC,
    _monitor_rectangle: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitor_vector = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };

    let mut monitor_info_ex = MONITORINFOEXW::default();
    monitor_info_ex.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    let is_true =
        unsafe { GetMonitorInfoW(monitor_handle, &mut monitor_info_ex.monitorInfo).as_bool() };
    if is_true {
        let rc_monitor = &monitor_info_ex.monitorInfo.rcMonitor;
        let rc_work = &monitor_info_ex.monitorInfo.rcWork;

        let current_monitor_info = MonitorInfo {
            x: rc_monitor.left,
            y: rc_monitor.top,

            // Monitor width/heigh
            width: rc_monitor.right - rc_monitor.left,
            height: rc_monitor.bottom - rc_monitor.top,

            // Work are top-left X/Y
            work_x: rc_work.left,
            work_y: rc_work.top,

            // Work area width/height
            work_width: rc_work.right - rc_work.left,
            work_height: rc_work.bottom - rc_work.top,
        };
        monitor_vector.push(current_monitor_info);
    }
    TRUE
}

thread_local! {
    /// Thread-local channel used by the low-level input hooks to forward captured
    /// events to the event-processing thread.
    ///
    /// Each hook callback runs on the thread that owns the Windows hooks. The
    /// thread-local sender allows those callbacks to send [`Events`] without
    /// requiring global mutable state or synchronization around a shared sender.
    ///
    /// The sender is initialized when the input-hook thread starts and is then
    /// accessed by [`mouse_hook`] and [`keyboard_hook`].
    pub static EVENT_TX: RefCell<Option<Sender<Events>>> =
        RefCell::new(None);
}

/// Captures global low-level mouse input and converts it into [`Events`].
///
/// This callback receives mouse events generated by the Windows
/// `WH_MOUSE_LL` hook. Relevant mouse messages are decoded from the supplied
/// Windows callback parameters and converted into the corresponding [`Events`]
/// variant.
///
/// Mouse coordinates are taken directly from [`MSLLHOOKSTRUCT::pt`] and
/// therefore represent screen coordinates. For wheel events, the wheel delta
/// is extracted from [`MSLLHOOKSTRUCT::mouseData`].
///
/// Injected mouse events are ignored so that events generated programmatically
/// by the application are not forwarded back through the input pipeline.
///
/// Supported mouse messages are:
///
/// - `WM_MOUSEMOVE`
/// - `WM_LBUTTONDOWN`
/// - `WM_LBUTTONUP`
/// - `WM_RBUTTONDOWN`
/// - `WM_RBUTTONUP`
/// - `WM_MBUTTONDOWN`
/// - `WM_MBUTTONUP`
/// - `WM_MOUSEWHEEL`
///
/// # Arguments
///
/// * `n_code` - Hook callback code supplied by Windows. Input processing occurs
///   only when this value is greater than or equal to `HC_ACTION`.
/// * `w_param` - Identifies the mouse message that triggered the callback.
/// * `l_param` - Pointer to an [`MSLLHOOKSTRUCT`] containing mouse position,
///   wheel information, and event flags.
///
/// # Returns
///
/// Returns the result of [`CallNextHookEx`] so that the next hook in the
/// low-level mouse hook chain can process the event.
///
/// # Safety
///
/// The `l_param` value must point to a valid [`MSLLHOOKSTRUCT`] when `n_code`
/// indicates an actionable hook event. The pointer is supplied and owned by
/// Windows and must not be retained after the callback returns.
pub unsafe extern "system" fn mouse_hook(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= HC_ACTION as i32 {
        // Decode and unpack x, y, delta from recieved lparam
        let (x, y, delta) = unsafe {
            let info = &*(l_param.0 as *const MSLLHOOKSTRUCT);

            // Ignore events that are not related to mouse (are injected) by starting next iteration
            if info.flags & LLMHF_INJECTED != 0 {
                return CallNextHookEx(Some(HHOOK::default()), n_code, w_param, l_param);
            }

            (info.pt.x, info.pt.y, (info.mouseData >> 16) as i16)
        };

        let event = match w_param.0 as u32 {
            WM_MOUSEMOVE => Some(Events::Move { x, y }),

            WM_LBUTTONDOWN => Some(Events::LeftDown { x, y }),

            WM_LBUTTONUP => Some(Events::LeftUp { x, y }),

            WM_RBUTTONDOWN => Some(Events::RightDown { x, y }),

            WM_RBUTTONUP => Some(Events::RightUp { x, y }),

            WM_MBUTTONDOWN => Some(Events::MiddleDown { x, y }),

            WM_MBUTTONUP => Some(Events::MiddleUp { x, y }),

            WM_MOUSEWHEEL => {
                if delta != 0 {
                    Some(Events::Scroll { x, y, delta })
                } else {
                    None
                }
            }

            _ => None,
        };

        // Send event to channel
        if let Some(ev) = event {
            EVENT_TX.with(|tx| {
                if let Some(tx) = tx.borrow().as_ref() {
                    let _ = tx.send(ev);
                }
            });
        }
    }

    unsafe { CallNextHookEx(Some(HHOOK::default()), n_code, w_param, l_param) }
}

/// Captures global low-level keyboard input and converts it into [`Events`].
///
/// This callback receives keyboard events generated by the Windows
/// `WH_KEYBOARD_LL` hook. The virtual-key code is extracted from
/// [`KBDLLHOOKSTRUCT::vkCode`] and converted into either [`Events::KeyDown`] or
/// [`Events::KeyUp`].
///
/// Injected keyboard events are ignored so that keyboard messages generated
/// programmatically are not forwarded through the input pipeline again.
///
/// # Arguments
///
/// * `n_code` - Hook callback code supplied by Windows. Input processing occurs
///   only when this value is greater than or equal to `HC_ACTION`.
/// * `w_param` - Identifies the keyboard message that triggered the callback.
/// * `l_param` - Pointer to a [`KBDLLHOOKSTRUCT`] containing the virtual-key
///   code and keyboard event flags.
///
/// # Supported Events
///
/// The following keyboard messages are converted into [`Events`]:
///
/// - `WM_KEYDOWN` → [`Events::KeyDown`]
/// - `WM_KEYUP` → [`Events::KeyUp`]
///
/// # Returns
///
/// Returns the result of [`CallNextHookEx`] so that the next hook in the
/// low-level keyboard hook chain can process the event.
///
/// # Safety
///
/// The `l_param` value must point to a valid [`KBDLLHOOKSTRUCT`] when `n_code`
/// indicates an actionable hook event. The pointer is supplied and owned by
/// Windows and must not be retained after the callback returns.
pub unsafe extern "system" fn keyboard_hook(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= HC_ACTION as i32 {
        let info = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };

        // Ignore events that are not related to keyboard (are injected) by starting next iteration
        if info.flags.0 & LLKHF_INJECTED.0 != 0 {
            return unsafe {
                CallNextHookEx(
                    Some(windows::Win32::UI::WindowsAndMessaging::HHOOK::default()),
                    n_code,
                    w_param,
                    l_param,
                )
            };
        }

        let vk = info.vkCode;

        let event = match w_param.0 as u32 {
            WM_KEYDOWN => Some(Events::KeyDown { vk }),

            WM_KEYUP => Some(Events::KeyUp { vk }),

            _ => None,
        };

        // Send event to channel
        if let Some(ev) = event {
            EVENT_TX.with(|tx| {
                if let Some(tx) = tx.borrow().as_ref() {
                    let _ = tx.send(ev);
                }
            });
        }
    }

    unsafe {
        CallNextHookEx(
            Some(windows::Win32::UI::WindowsAndMessaging::HHOOK::default()),
            n_code,
            w_param,
            l_param,
        )
    }
}
