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
        UI::{
            HiDpi::{PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness},
            Input::KeyboardAndMouse::{
                VIRTUAL_KEY, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
            },
            Shell::PathFindFileNameW,
            WindowsAndMessaging::*,
        },
    },
    core::{Error as WinErr, PCWSTR, PWSTR, w},
};

use crate::platform::windows::{
    models::{
        DesktopHandle, FullscrennOcclusionData, GlobalVariables, MonitorInfo, Vector2Platform,
    },
    procs::{enum_windows_proc, fullscreen_window_enum_proc, monitor_enum_proc},
};

pub fn initialize() -> Result<GlobalVariables, WinErr> {
    unsafe {
        // Set the process DPI awareness to get physical pixel coordinates.
        // This must be done before any windows are created.
        let dpi_awarenexx_result = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
        if dpi_awarenexx_result.is_err() {
            // Continue if needed, but coordinate values may be scaled.
        }

        // Create global variables object at initialization then pass it as return type
        let mut global_variables = GlobalVariables::default();

        // Locate the Progman window (the desktop window)
        global_variables.progman_window_handle = Some(FindWindowW(w!("Progman"), None)?);

        // Send message 0x052C to Progman to force creation of a WorkerW window
        let mut result = 0;
        SendMessageTimeoutW(
            global_variables.progman_window_handle.unwrap(),
            0x052c,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            Some(&mut result),
        );

        // Try to locate the Shell view (desktop icons) and WorkerW child directly under Progman
        global_variables.shell_view_whidow_handle = FindWindowExW(
            global_variables.progman_window_handle,
            None,
            w!("SHELLDLL_DefView"),
            None,
        )
        .ok();

        global_variables.workerw_window_handle = Some(
            FindWindowExW(
                global_variables.progman_window_handle,
                None,
                w!("WorkerW"),
                None,
            )
            .map_err(|e| {
                // Fallback for pre-24H2 builds where the WorkerW is a sibling window
                global_variables.is_pre_24h2 = true;
                let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));
                e
            })?,
        );
        Ok(global_variables)
    }
}

pub fn cleanup(g: &mut GlobalVariables) {
    const MAX_PATH: u32 = 260_u32;
    if g.engine_window_handle.is_some() {
        // Restore the desktop wallpaper
        let mut wallpaper_path = [0_u16; MAX_PATH as usize];
        unsafe {
            let result = SystemParametersInfoW(
                SPI_GETDESKWALLPAPER,
                MAX_PATH,
                Some(wallpaper_path.as_mut_ptr() as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
            if result.is_ok() {
                // Reapply the wallpaper to force a refresh
                let _ = SystemParametersInfoW(
                    SPI_SETDESKWALLPAPER,
                    0,
                    Some(wallpaper_path.as_mut_ptr() as *mut _),
                    SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
                );
            }
        }

        g.progman_window_handle = None;
        g.workerw_window_handle = None;
        g.shell_view_whidow_handle = None;
        g.engine_window_handle = None;
    }
}

pub fn enumerate_monitors(g: &mut GlobalVariables) -> Vec<MonitorInfo> {
    let mut monitor_info_vector: Vec<MonitorInfo> = Vec::new();
    let lparam = LPARAM(&mut monitor_info_vector as *mut Vec<MonitorInfo> as isize);
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(monitor_enum_proc), lparam) };
    // Convert to desktop coodrdinates starting at 0, 0
    g.desktop_x = i32::MAX;
    g.desktop_y = i32::MAX;

    for monitor in &monitor_info_vector {
        if monitor.x < g.desktop_x {
            g.desktop_x = monitor.x;
        }
        if monitor.y < g.desktop_y {
            g.desktop_y = monitor.y;
        }
    }

    for monitor in &mut monitor_info_vector {
        monitor.x -= g.desktop_x;
        monitor.y -= g.desktop_y;
    }

    monitor_info_vector
}

pub fn configure_wallpaper_window(hwnd: HWND, monitor: &MonitorInfo, g: &mut GlobalVariables) {
    g.engine_window_handle = Some(hwnd);

    if g.engine_window_handle.is_none() || g.progman_window_handle.is_none() {
        return;
    }

    if g.is_pre_24h2 {
        // Reparent the window to the custom WorkerW window.
        // This attaches the window as a child of your WorkerW,
        // which should place it behind desktop icons if your WorkerW is set up that way.
        unsafe {
            let _ = SetParent(g.engine_window_handle.unwrap(), g.workerw_window_handle);

            // Adjust window styles so that it behaves like a wallpaper.
            // For example, you may remove the title bar or border:
            let mut style = GetWindowLongPtrW(g.engine_window_handle.unwrap(), GWL_STYLE);
            style &= !WS_OVERLAPPEDWINDOW.0 as isize; // Remove common overlapped window styles.
            style |= WS_CHILD.0 as isize; // Make it a child window.
            SetWindowLongPtrW(g.engine_window_handle.unwrap(), GWL_STYLE, style);
        }
    } else {
        unsafe {
            // Prepare the engine window to be a layered child of Progman
            let mut style = GetWindowLongPtrW(g.engine_window_handle.unwrap(), GWL_STYLE);
            style &= !WS_OVERLAPPEDWINDOW.0 as isize; // Remove decorations
            style |= WS_CHILD.0 as isize; // Child style required for SetParent
            SetWindowLongPtrW(g.engine_window_handle.unwrap(), GWL_STYLE, style);

            let mut ex_style = GetWindowLongPtrW(g.engine_window_handle.unwrap(), GWL_EXSTYLE);
            ex_style |= WS_EX_LAYERED.0 as isize; // Make it a layered window for 24h2
            SetWindowLongPtrW(g.engine_window_handle.unwrap(), GWL_EXSTYLE, ex_style);

            // Reparent the engine window directly to Progman
            let _ = SetParent(g.engine_window_handle.unwrap(), g.progman_window_handle);

            // Ensure correct Z-order: below icons but above the system wallpaper
            if g.shell_view_whidow_handle.is_some() {
                let _ = SetWindowPos(
                    g.engine_window_handle.unwrap(),
                    g.shell_view_whidow_handle,
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
            g.engine_window_handle.unwrap(),
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

pub fn get_wallpaper_target(g: &mut GlobalVariables, monitor_index: i32) -> MonitorInfo {
    let monitors = enumerate_monitors(g);

    if monitor_index < 0 || monitor_index as usize >= monitors.len() {
        let mut info = MonitorInfo::default();
        info.x = 0;
        info.y = 0;
        unsafe {
            info.width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            info.height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        }
        info
    } else {
        monitors[monitor_index as usize].clone()
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
    global_variable: &mut GlobalVariables,
) -> bool {
    let mut occlusion_data = FullscrennOcclusionData::default();
    let wrapped = &mut (global_variable, &mut occlusion_data)
        as *mut (&mut GlobalVariables, &mut FullscrennOcclusionData) as isize;
    let lparam = LPARAM(wrapped);

    let _ = unsafe { EnumWindows(Some(fullscreen_window_enum_proc), lparam) };
    let occlusion_fraction =
        compute_occlution_fraction(&occlusion_data.occluded_rects, monitor, 100);
    occlusion_fraction >= threshold
}

pub fn update_mouse_state(g: &mut GlobalVariables) {
    // Save previous state
    g.previous_mouse_state
        .copy_from_slice(&g.current_mouse_state);

    let get_virtual_key_for_mouse_button = |button: usize| -> u16 {
        match button {
            0 => VK_LBUTTON,
            1 => VK_RBUTTON,
            2 => VK_MBUTTON,
            3 => VK_XBUTTON1,
            4 => VK_XBUTTON2,
            _ => VIRTUAL_KEY::default(),
        }
        .0
    };

    // Update current state
    (0..5).for_each(|i| {
        g.current_mouse_state[i] = match get_virtual_key_for_mouse_button(i) {
            0 => false,
            _ => true,
        }
    });
}

pub fn is_mouse_button_pressed(button: i32, g: &GlobalVariables) -> bool {
    if button < 0 || button >= 5 {
        return false;
    }
    let button = button as usize;
    g.current_mouse_state[button] && !g.previous_mouse_state[button]
}

pub fn is_mouse_button_donw(button: i32, g: &GlobalVariables) -> bool {
    if button < 0 || button >= 5 {
        return false;
    }
    g.current_mouse_state[button as usize]
}

pub fn is_mouse_button_released(button: i32, g: &GlobalVariables) -> bool {
    if button < 0 || button >= 5 {
        return false;
    }
    let button = button as usize;
    !g.current_mouse_state[button] && g.previous_mouse_state[button]
}

pub fn is_mouse_button_up(button: i32, g: &GlobalVariables) -> bool {
    if button < 0 && button >= 5 {
        return false;
    }
    !g.current_mouse_state[button as usize]
}

pub fn get_mouse_x(g: &GlobalVariables) -> i32 {
    let mut p = POINT::default();

    if get_relative_cursor_pos(&mut p, g) {
        return p.x;
    }
    0
}

pub fn get_mouse_y(g: &GlobalVariables) -> i32 {
    let mut p = POINT::default();

    if get_relative_cursor_pos(&mut p, g) {
        return p.y;
    }
    0
}

pub fn get_mouse_position(g: &GlobalVariables) -> Vector2Platform {
    let mut p = POINT::default();

    if get_relative_cursor_pos(&mut p, g) {
        return Vector2Platform {
            x: p.x as _,
            y: p.y as _,
        };
    }
    Vector2Platform { x: 0_f32, y: 0_f32 }
}

pub fn supports_dynamuc_wallpaper() -> bool {
    true
}

pub fn supports_multi_monito() -> bool {
    true
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

fn get_relative_cursor_pos(p: &mut POINT, g: &GlobalVariables) -> bool {
    unsafe {
        if GetCursorPos(p).is_err() {
            return false;
        }

        // Convert to desktop coordinates
        p.x -= g.desktop_x;
        p.y -= g.desktop_y;

        // Convert to window coordinates
        p.x -= g.selected_monitor.as_ref().unwrap().x;
        p.y -= g.selected_monitor.as_ref().unwrap().y;

        true
    }
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
