use windows::Win32::{
    Foundation::{HWND, RECT},
    System::StationsAndDesktops::{CloseDesktop, HDESK},
};

// To avoid using static global variables we use this structure to store them
// and pass it to fucntions as a refrence
#[derive(Default)]
pub struct GlobalVariables {
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
