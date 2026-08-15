use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
        Graphics::Gdi::EnumDisplayMonitors,
        System::StationsAndDesktops::{CloseDesktop, HDESK},
        UI::{
            HiDpi::{PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness},
            Input::KeyboardAndMouse::{
                VIRTUAL_KEY, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
            },
            WindowsAndMessaging::{
                EnumWindows, FindWindowExW, FindWindowW, GetCaretPos, SMTO_NORMAL,
                SPI_GETDESKWALLPAPER, SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageTimeoutW, SystemParametersInfoW,
            },
        },
    },
    core::{Error as WinErr, w},
};

use crate::platform::windows::procs::{enum_windows_proc, monitor_enum_proc};

// use crate::platform::windows::procs::monitor_enum_proc;

// To avoid using static global variables we use this structure to store them
// and pass it to fucntions as a refrence
#[derive(Default)]
pub struct WindowsPlatform {
    // Global variables to hold handles within the desktop hierarchy
    // g_progmanWindowHandle : top level Program Manager window
    // g_workerWindowHandle  : child WorkerW window rendering the static wallpaper
    // g_shellViewWindowHandle: child ListView window displaying the desktop icons
    // g_engineWindowHandle  : handle to the engine window we inject
    pub progman_window_handle: Option<HWND>,
    pub workerw_window_handle: Option<HWND>,
    pub shell_view_whidow_handle: Option<HWND>,
    pub engine_window_handle: Option<HWND>,

    // Current monitor in desktop coordinates
    pub selected_monitor: Option<MonitorInfo>,

    // The offset to the desktop coordinates
    // Windows desktop coordinates start at the top left of the primary monitor
    // Subtract this offset to get the desktop coordinates
    pub desktop_x: i32,
    pub desktop_y: i32,

    //Mouse state tracking
    pub current_mouse_state: [bool; 5],
    pub previous_mouse_state: [bool; 5],

    pub is_pre_24h2: bool,
}
impl WindowsPlatform {
    pub fn get_mouse_y(&self) -> i32 {
        let mut p = POINT::default();

        if Self::get_relative_cursor_pos(self, &mut p) {
            return p.y;
        }
        0
    }
    pub fn initialize(&mut self) -> Result<(), WinErr> {
        unsafe {
            // Set the process DPI awareness to get physical pixel coordinates.
            // This must be done before any windows are created.
            let dpi_awarenexx_result = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
            if dpi_awarenexx_result.is_err() {
                // Continue if needed, but coordinate values may be scaled.
            }

            // Create global variables object at initialization then pass it as return type

            // Locate the Progman window (the desktop window)
            self.progman_window_handle = Some(FindWindowW(w!("Progman"), None)?);

            // Send message 0x052C to Progman to force creation of a WorkerW window
            let mut result = 0;
            SendMessageTimeoutW(
                self.progman_window_handle.unwrap(),
                0x052c,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut result),
            );

            // Try to locate the Shell view (desktop icons) and WorkerW child directly under Progman
            self.shell_view_whidow_handle = FindWindowExW(
                self.progman_window_handle,
                None,
                w!("SHELLDLL_DefView"),
                None,
            )
            .ok();

            self.workerw_window_handle = Some(
                FindWindowExW(self.progman_window_handle, None, w!("WorkerW"), None).map_err(
                    |e| {
                        // Fallback for pre-24H2 builds where the WorkerW is a sibling window
                        self.is_pre_24h2 = true;
                        let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));
                        e
                    },
                )?,
            );
            Ok(())
        }
    }

    pub fn enumerate_monitors(&mut self) -> Vec<MonitorInfo> {
        let mut monitor_info_vector: Vec<MonitorInfo> = Vec::new();
        let lparam = LPARAM(&mut monitor_info_vector as *mut Vec<MonitorInfo> as isize);
        let _ = unsafe { EnumDisplayMonitors(None, None, Some(monitor_enum_proc), lparam) };

        // Convert to desktop coodrdinates starting at 0, 0
        self.desktop_x = i32::MAX;
        self.desktop_y = i32::MAX;

        for monitor in &monitor_info_vector {
            if monitor.x < self.desktop_x {
                self.desktop_x = monitor.x;
            }
            if monitor.y < self.desktop_y {
                self.desktop_y = monitor.y;
            }
        }

        for monitor in &mut monitor_info_vector {
            monitor.x -= self.desktop_x;
            monitor.y -= self.desktop_y;
        }

        monitor_info_vector
    }

    pub fn get_mouse_position(&self) -> Vector2Platform {
        let mut p = POINT::default();

        if Self::get_relative_cursor_pos(self, &mut p) {
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
    pub fn is_mouse_button_pressed(&self, button: i32) -> bool {
        if button < 0 || button >= 5 {
            return false;
        }
        let button = button as usize;
        self.current_mouse_state[button] && !self.previous_mouse_state[button]
    }

    pub fn is_mouse_button_donw(&self, button: i32) -> bool {
        if button < 0 || button >= 5 {
            return false;
        }
        self.current_mouse_state[button as usize]
    }

    pub fn is_mouse_button_released(&self, button: i32) -> bool {
        if button < 0 || button >= 5 {
            return false;
        }
        let button = button as usize;
        !self.current_mouse_state[button] && self.previous_mouse_state[button]
    }

    pub fn is_mouse_button_up(&self, button: i32) -> bool {
        if button < 0 && button >= 5 {
            return false;
        }
        !self.current_mouse_state[button as usize]
    }

    pub fn get_mouse_x(&self) -> i32 {
        let mut p = POINT::default();

        if Self::get_relative_cursor_pos(self, &mut p) {
            return p.x;
        }
        0
    }

    fn get_relative_cursor_pos(&self, p: &mut POINT) -> bool {
        unsafe {
            if GetCaretPos(p).is_err() {
                return false;
            }

            // Convert to desktop coordinates
            p.x -= self.desktop_x;
            p.y -= self.desktop_y;

            // Convert to window coordinates
            p.x -= self.selected_monitor.as_ref().unwrap().x;
            p.y -= self.selected_monitor.as_ref().unwrap().y;

            true
        }
    }

    pub fn cleanup(&mut self) {
        const MAX_PATH: u32 = 260_u32;
        if self.engine_window_handle.is_some() {
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

            self.progman_window_handle = None;
            self.workerw_window_handle = None;
            self.shell_view_whidow_handle = None;
            self.engine_window_handle = None;
        }
    }

    pub fn update_mouse_state(&mut self) {
        // Save previous state
        self.previous_mouse_state
            .copy_from_slice(&self.current_mouse_state);

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
            self.current_mouse_state[i] = match get_virtual_key_for_mouse_button(i) {
                0 => false,
                _ => true,
            }
        });
    }
}
#[derive(Debug, Default, Clone)]
pub struct MonitorInfo {
    pub x: i32, // X coordinate of the monitor's top-left corner
    pub y: i32, // Y coordinate of the monitor's top-left corner

    pub width: i32,  // Monitor width in pixels
    pub height: i32, // Monitor height in pixels

    pub work_width: i32,  // Work area width
    pub work_height: i32, // Work area height
}

#[derive(Debug, Default)]
pub struct FullscrennOcclusionData {
    pub monitor: MonitorInfo,
    pub occluded_rects: Vec<RECT>,
}

// Vector2 structure to avoid engine dependency in this header
pub struct Vector2Platform {
    pub x: f32,
    pub y: f32,
}

pub struct DesktopHandle(pub HDESK);

impl Drop for DesktopHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseDesktop(self.0);
        }
    }
}
