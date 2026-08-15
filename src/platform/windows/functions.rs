// use std::os::windows;

use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
            Gdi::*,
        },
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
    core::{PCWSTR, PWSTR},
};

use crate::platform::windows::{
    models::{DesktopHandle, FullscreennOcclusionData, MonitorInfo, WindowsPlatform},
    procs::fullscreen_window_enum_proc,
};

pub fn configure_wallpaper_window(hwnd: HWND, monitor: &MonitorInfo, g: &mut WindowsPlatform) {
    let _ = match g.progman_window_handle {
        Some(hwnd) => hwnd,
        None => return,
    };

    g.engine_window_handle = Some(hwnd);

    if g.is_pre_24h2 {
        // Reparent the window to the custom WorkerW window.
        // This attaches the window as a child of your WorkerW,
        // which should place it behind desktop icons if your WorkerW is set up that way.
        unsafe {
            let _ = SetParent(hwnd, g.workerw_window_handle);

            // Adjust window styles so that it behaves like a wallpaper.
            // For example, you may remove the title bar or border:
            let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            style &= !WS_OVERLAPPEDWINDOW.0 as isize; // Remove common overlapped window styles.
            style |= WS_CHILD.0 as isize; // Make it a child window.
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        }
    } else {
        unsafe {
            // Prepare the engine window to be a layered child of Progman
            let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            style &= !WS_OVERLAPPEDWINDOW.0 as isize; // Remove decorations
            style |= WS_CHILD.0 as isize; // Child style required for SetParent
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);

            let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            ex_style |= WS_EX_LAYERED.0 as isize; // Make it a layered window for 24h2
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);

            // Reparent the engine window directly to Progman
            let _ = SetParent(hwnd, g.progman_window_handle);

            // Ensure correct Z-order: below icons but above the system wallpaper
            if g.shell_view_window_handle.is_some() {
                let _ = SetWindowPos(
                    hwnd,
                    g.shell_view_window_handle,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                );
            }
            if g.workerw_window_handle.is_some() {
                let _ = SetWindowPos(
                    g.workerw_window_handle.unwrap(),
                    g.engine_window_handle,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                );
            }
        }
    }

    unsafe {
        // Reparent the engine window to WorkerW
        g.selected_monitor = Some(monitor.clone());

        // Resize/reposition the engine window to match its new parent.
        // g_progmanWindowHandle spans the entire virtual desktop in modern builds
        let _ = SetWindowPos(
            hwnd,
            None,
            monitor.x,
            monitor.y,
            monitor.width,
            monitor.height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );

        let _ = RedrawWindow(
            g.engine_window_handle,
            None,
            None,
            RDW_INVALIDATE | RDW_UPDATENOW,
        );
    }
}

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

        let Ok(proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };

        let mut path = [0_u16; 260];
        let mut len = path.len() as u32;

        if QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_FORMAT(0),
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
        .is_err()
        {
            return false;
        };

        let filename = PathFindFileNameW(PWSTR(path.as_mut_ptr()));
        let filename = filename.as_wide();

        String::from_utf16_lossy(filename).eq_ignore_ascii_case("LockApp.exe")
    }
}

pub fn is_monitor_occluded(
    monitor: &MonitorInfo,
    threshold: f64,
    global_variable: &mut WindowsPlatform,
) -> bool {
    let mut occlusion_data = FullscreennOcclusionData::default();
    let wrapped = &mut (global_variable, &mut occlusion_data)
        as *mut (&mut WindowsPlatform, &mut FullscreennOcclusionData) as isize;
    let lparam = LPARAM(wrapped);

    let _ = unsafe { EnumWindows(Some(fullscreen_window_enum_proc), lparam) };
    let occlusion_fraction =
        compute_occlution_fraction(&occlusion_data.occluded_rects, monitor, 100);
    occlusion_fraction >= threshold
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

fn compute_occlution_fraction(
    occluded_rects: &Vec<RECT>,
    monitor: &MonitorInfo,
    sample_step: i32,
) -> f64 {
    let mut occluded_count = 0;
    let mut total_samples = 0;

    let mut y = monitor.y;
    while y < monitor.y + monitor.height {
        let mut x = monitor.x;
        while x < monitor.x + monitor.width {
            total_samples += 1;
            let mut is_occluded = false;

            for rect in occluded_rects {
                if x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom {
                    is_occluded = true;
                    break;
                }
            }
            if is_occluded {
                occluded_count += 1;
            }
            x += sample_step;
        }
        y += sample_step;
    }
    if total_samples == 0 {
        return 0.0;
    }
    return occluded_count as f64 / total_samples as f64;
}

pub fn is_invisible_win10_background_app_window(hwnd: HWND) -> bool {
    let mut cloaked_val = 0;
    let pvattribute = &mut cloaked_val as *mut _ as *mut std::ffi::c_void;
    let hres = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            pvattribute,
            size_of_val(&cloaked_val) as u32,
        )
    };
    hres.is_ok() && cloaked_val != 0
}

fn is_secure_desktop() -> bool {
    unsafe {
        let desktop = match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) {
            Ok(handle) => DesktopHandle(handle),
            Err(_) => return true, // couldn’t query ⇒ assume we’re secure to be conservative
        };
        let mut bytes = 0_u32;
        let result =
            GetUserObjectInformationW(HANDLE(desktop.0.0), UOI_NAME, None, 0, Some(&mut bytes));
        if result.is_ok() && GetLastError() != ERROR_INSUFFICIENT_BUFFER {
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
