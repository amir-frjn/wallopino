use windows::{
    Win32::{
        Foundation::{FALSE, HWND, LPARAM, RECT, TRUE},
        Graphics::Gdi::{
            GetMonitorInfoW, HDC, HMONITOR, IntersectRect, MONITORINFO, MONITORINFOEXW,
        },
        UI::WindowsAndMessaging::{
            FindWindowExW, GetClassNameW, GetShellWindow, GetWindowRect, IsIconic, IsWindowVisible,
        },
    },
    core::{BOOL, w},
};

use crate::{
    is_invisible_win10_background_app_window,
    platform::windows::models::{FullscreennOcclusionData, MonitorInfo, WindowsPlatform},
};

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0_u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len])
}

pub extern "system" fn fullscreen_window_enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let wrapped =
            &mut *(lparam.0 as *mut (&mut WindowsPlatform, &mut FullscreennOcclusionData));
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

        let sz_class_name = class_name(hwnd);
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

// Callback function for EnumWindows to locate the proper WorkerW window
// To avoid using global variable we pass them as a WindowsPlatform via lparam
pub extern "system" fn enum_windows_proc(window_handle: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        // println!("here");
        // Look for a child window named "SHELLDLL_DefView" in each top-level window.
        if let Ok(shell_view_window) =
            FindWindowExW(window_handle.into(), None, w!("SHELLDLL_DefView"), None)
        {
            if !shell_view_window.is_invalid() {
                // If found, get the WorkerW window that is a sibling of the found window.
                let g = { &mut *(lparam.0 as *mut WindowsPlatform) };
                g.workerw_window_handle =
                    { FindWindowExW(None, Some(window_handle), w!("WorkerW"), None) }.ok();
                return FALSE;
            }
        }
    }
    TRUE
}

// Monitor enumeration callback
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
