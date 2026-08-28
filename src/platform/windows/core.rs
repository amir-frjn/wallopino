// AI is used to generate documentation and comments for the code.

//! Windows platform implementation for wallpaper engine functionality.
//! This piece of code is inspired by LuminWallpaper library: <https://github.com/jensroth-git/LuminWallpaper>
//!
//! This module provides Windows-specific implementations to set a HWND as desktop wallpaper.
//!
//! # Overview
//!
//! The core functionality is provided by [`AttachWindow`], which handles:
//! - Discovering the Windows desktop window hierarchy (`Progman`, `WorkerW`, `SHELLDLL_DefView`)
//! - Attaching and configuring windows as wallpapers
//! - Multi-monitor support with coordinate normalization
//! - Cleanup and wallpaper restoration
//! - Monitoring z-order changes that cause the engine to be hidden behind the desktop background and restoring its correct position.
//!
//! # Platform Support
//!
//! This module targets **Windows 10 and later**, with specific code paths for:
//! - **Pre-Windows 24H2**: Uses `WorkerW` window reparenting
//! - **Windows 24H2 and later**: Uses direct `Progman` reparenting with layered windows
//!
//! # Basic Usage
//!
//! ```no_run
//! # use windows::Win32::Foundation::HWND;
//! # use windows::core::Error;
//! # fn example(hwnd: HWND) -> Result<(), Error> {
//! use crate::platform::windows::AttachWindow;
//!
//! // Initialize the platform
//! let mut platform = AttachWindow::initialize()?;
//!
//! // Get target monitor (virtual desktop or specific monitor)
//! let monitor = platform.get_wallpaper_target(None)?;
//!
//! // Configure the window as wallpaper
//! platform.configure_wallpaper_window(hwnd, &monitor, false)?;
//!
//! // Update mouse state each frame
//!
//!
//! // These steps can simply done using:
//! let mut platform = AttachWindow::auto_attach(hwnd)?;
//!
//! // Watch for z-order changes that cause hiding and restore it automatically
//! platform.start_watcher(std::time::Duration::from_millis(100));
//!
//!/*... */
//!
//! // Cleanup on shutdown
//! platform.cleanup()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Architecture
//!
//! The Windows desktop hierarchy consists of:
//! 1. **Progman** – The top-level Program Manager window
//! 2. **SHELLDLL_DefView** – Child window containing desktop icons
//! 3. **WorkerW** – Container window behind the icons (where static wallpapers are rendered)
//!
//! This module navigates this hierarchy to inject custom wallpaper windows at the
//! correct Z-order position (below icons, above the static wallpaper).
//!
//! For monitoring, it spawns a thread that checks whether a `WorkerW` window
//! exists between our engine and `SHELLDLL_DefView`. If one is found, the engined
//! needs to be reattached.
//!
//! # Features
//!
//! - ✅ Full multi-monitor support
//! - ✅ Dynamic wallpaper support
//! - ✅ Mouse input tracking (5 buttons: left, right, middle, X1, X2)
//! - ✅ Edge-triggered and state-triggered mouse events
//! - ✅ Automatic DPI awareness
//! - ✅ Graceful handling of Windows version differences
//! - ✅ Automatic z-order monitoring and recovery
//!
//! # Safety
//!
//! This module contains `unsafe` code for Windows API calls. The caller should ensure:
//! - Valid window handles (`HWND`) are provided
//! - The calling context has appropriate permissions
//! - The application is running on a supported Windows version
//!
//! # See Also
//!
//! - [`AttachWindow`] – Main struct for platform operations
//! - [`MonitorInfo`] – Display monitor information
//! - [`Vector2Platform`] – 2D vector for mouse position
use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use windows::{
    Win32::{
        Foundation::{COLORREF, CloseHandle, HANDLE, HWND, LPARAM, POINT, RECT, WPARAM},
        Graphics::Gdi::{EnumDisplayMonitors, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow},
        System::StationsAndDesktops::{CloseDesktop, HDESK},
        UI::{
            HiDpi::{PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness},
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
            },
            WindowsAndMessaging::{
                EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetCursorPos,
                GetSystemMetrics, GetWindowLongPtrW, LWA_ALPHA, SM_CXEDGE, SM_CXVIRTUALSCREEN,
                SM_CYEDGE, SM_CYVIRTUALSCREEN, SMTO_NORMAL, SPI_GETDESKWALLPAPER,
                SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SWP_NOACTIVATE,
                SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
                SendMessageTimeoutW, SetLayeredWindowAttributes, SetParent, SetWindowLongPtrW,
                SetWindowPos, SystemParametersInfoW, WS_CHILD, WS_EX_LAYERED, WS_EX_STATICEDGE,
                WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{Error as WinErr, w},
};

use crate::platform::windows::{
    functions::has_workerw_between,
    procs::{enum_windows_proc, monitor_enum_proc},
};

// To avoid using static global variables we use this structure to store them
// and pass it to fucntions as a refrence
#[derive(Default, Debug)]
pub struct AttachWindow {
    // Global variables to hold handles within the desktop hierarchy
    // g_progmanWindowHandle : top level Program Manager window
    // g_workerWindowHandle  : child WorkerW window rendering the static wallpaper
    // g_shellViewWindowHandle: child ListView window displaying the desktop icons
    // g_engineWindowHandle  : handle to the engine window we inject
    pub progman_window_handle: Option<HWND>,
    pub workerw_window_handle: Option<HWND>,
    pub shell_view_window_handle: Option<HWND>,
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

    // This is to send kill signal to watcher thread if exist
    watcher_kill_flag: Option<Arc<AtomicBool>>,
    join_handle: Option<JoinHandle<Result<(), WinErr>>>,
}
impl AttachWindow {
    /// Initializes the platform-specific state required to attach a window to the background.
    ///
    /// This method performs essential setup steps to locate the Windows desktop window hierarchy:
    /// - Sets the process DPI awareness to `PROCESS_PER_MONITOR_DPI_AWARE` for accurate physical
    ///   pixel coordinates (continues even if this fails, though coordinates may be scaled).
    /// - Locates the `Progman` window (the desktop window).
    /// - Sends a `0x052C` message to `Progman` to force creation of a `WorkerW` container window.
    /// - Attempts to find the `SHELLDLL_DefView` (desktop icons view) and `WorkerW` windows as
    ///   direct children of `Progman`.
    /// - If `WorkerW` is not found, falls AttachWindow to enumerating all windows via `EnumWindows` to
    ///   locate it (used for pre-Windows 24H2 systems).
    ///
    /// # Returns
    /// Returns `Ok(Self)` with the initialized platform state on success, or a
    /// [`windows::core::Error`] (Win32 error) if initialization fails.
    ///
    /// # Errors
    /// This function will return an error if:
    /// - The `Progman` window cannot be found.
    /// - The `EnumWindows` call fails (when fallAttachWindow is needed).
    /// - The `WorkerW` window cannot be located after all attempts.
    ///
    /// # Platform Support
    /// This function is Windows-specific and requires access to the Windows API.
    ///
    /// # Safety
    /// This function contains `unsafe` blocks for Windows API calls. The caller must ensure
    /// that the process has the necessary permissions and that the calling context is valid.
    ///
    /// # Example
    /// ```
    /// let hwnd = HEWND::new();
    /// let mut window_platform = AttachWindow::initialize()?;
    /// let monitor = window_platform.get_wallpaper_target(None)?;
    /// window_platform.configure_wallpaper_window(hwnd, &monitor)?;
    ///```
    pub fn initialize() -> Result<Self, WinErr> {
        let mut windows_platform = Self::default();
        unsafe {
            // Set the process DPI awareness to get physical pixel coordinates.
            // This must be done before any windows are created.
            let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);

            // Locate the Progman window (the desktop window)
            windows_platform.progman_window_handle = Some(FindWindowW(w!("Progman"), None)?);

            // Send message 0x052C to Progman to force creation of a WorkerW window
            let mut result = 0;
            SendMessageTimeoutW(
                windows_platform.progman_window_handle.unwrap(),
                0x052c,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut result),
            );

            // Try to locate the Shell view (desktop icons) and WorkerW child directly under Progman
            windows_platform.shell_view_window_handle = FindWindowExW(
                windows_platform.progman_window_handle,
                None,
                w!("SHELLDLL_DefView"),
                None,
            )
            .ok();

            windows_platform.workerw_window_handle = FindWindowExW(
                windows_platform.progman_window_handle,
                None,
                w!("WorkerW"),
                None,
            )
            .ok();

            if windows_platform.workerw_window_handle.is_none() {
                windows_platform.is_pre_24h2 = true;
                EnumWindows(
                    Some(enum_windows_proc),
                    LPARAM(&mut windows_platform as *mut _ as _),
                )?;
            }
            if windows_platform.workerw_window_handle.is_none() {
                return Err(WinErr::empty());
            }
        }
        Ok(windows_platform)
    }

    /// Configures the wallpaper window by attaching it to the desktop background with proper styling and positioning.
    ///
    /// This method performs platform-specific window configuration to embed the provided window handle
    /// into the Windows desktop hierarchy. The behavior differs based on the Windows version:
    ///
    /// ## Windows 24H2 and later
    /// - Removes window decorations and applies the `WS_CHILD` style.
    /// - Enables layered window attributes (`WS_EX_LAYERED`) for proper blending.
    /// - Reparents the window directly to the `Progman` window.
    /// - Positions the window below the desktop icons but above the system wallpaper using Z-order adjustments.
    ///
    /// ## Pre-Windows 24H2
    /// - Re-parents the window to the `WorkerW` container window.
    /// - Removes title bar and border styles, applies `WS_CHILD` style.
    /// - This places the window behind desktop icons while maintaining proper layering.
    ///
    /// # Parameters
    /// - `hwnd`: The window handle to configure as a wallpaper.
    /// - `monitor`: The monitor information containing position and dimensions for the window.
    /// - `static_edge_mode`: Some Windows windows generated by frameworks such as Tao may
    ///   detach from the desktop background when they receive mouse-click events. To prevent
    ///   this behavior, enable this flag to add the `WS_EX_STATICEDGE` style to the HWND.
    ///   However, `WS_EX_STATICEDGE` adds a visible border around the window, to prevent this
    ///   side effect it will be zoomed
    ///
    /// # Returns
    /// Returns `Ok(())` on success, or a [`windows::core::Error`] (Win32 error) if any operation fails.
    ///
    /// # Errors
    /// This function will return an error if:
    /// - Any Windows API call (`SetParent`, `SetWindowLongPtrW`, `SetLayeredWindowAttributes`, etc.) fails.
    /// - Window style or position modifications cannot be applied.
    /// - Redrawing the window fails.
    ///
    /// # Notes
    /// - If `Progman` window is not available (i.e., not initialized), the function returns `Ok(())` as a no-op.
    /// - The window is automatically resized to match the monitor dimensions provided.
    /// - For 24H2 systems, the Z-order is carefully managed to ensure the window is:
    ///   1. Below desktop icons (`SHELLDLL_DefView`)
    ///   2. Above the `WorkerW` container (which holds the system wallpaper)
    ///
    /// # Safety
    /// This function contains `unsafe` blocks for Windows API calls. The caller must ensure:
    /// - The `hwnd` is a valid window handle (and isize number that will cast into HWND automatically).
    /// - The window is not already destroyed or in an invalid state.
    /// - The process has appropriate permissions for window manipulation.
    ///
    /// # Example
    /// ```
    /// let mut platform = AttachWindow::initialize()?;
    /// let monitor = platform.get_wallpaper_target(None)?;
    /// platform.configure_wallpaper_window(hwnd, &monitor, false)?;
    /// ```
    pub fn configure_wallpaper_window(
        &mut self,
        hwnd: isize,
        monitor: &MonitorInfo,
        static_edge_mode: bool,
    ) -> Result<(), WinErr> {
        let hwnd = HWND(hwnd as _);

        self.engine_window_handle = Some(hwnd);

        if self.progman_window_handle.is_none() {
            return Ok(());
        }

        unsafe {
            if self.is_pre_24h2 {
                // Re-parent the window to the custom WorkerW window.
                // This attaches the window as a child of your WorkerW,
                // which should place it behind desktop icons if your WorkerW is set up that way.
                SetParent(hwnd, self.workerw_window_handle)?;

                // Adjust window styles so that it behaves like a wallpaper.
                // For example, you may remove the title bar or border:
                let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                style &= !WS_OVERLAPPEDWINDOW.0 as isize; // Remove common overlapped window styles.
                style |= WS_CHILD.0 as isize; // Make it a child window.
                SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            } else {
                // Prepare the engine window to be a layered child of Progman
                let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                style &= !WS_OVERLAPPEDWINDOW.0 as isize; // Remove decorations
                style |= WS_CHILD.0 as isize; // Child style required for SetParent
                SetWindowLongPtrW(hwnd, GWL_STYLE, style);

                let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                ex_style |= WS_EX_LAYERED.0 as isize; // Make it a layered window for 24h2

                // Prevents the window from detaching when clicked (in some cases), but adds a border.
                if static_edge_mode {
                    ex_style |= WS_EX_STATICEDGE.0 as isize;
                }

                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
                SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;

                // Reparent the engine window directly to Progman
                SetParent(hwnd, self.progman_window_handle)?;

                // Ensure correct Z-order: below icons but above the system wallpaper
                if self.shell_view_window_handle.is_some() {
                    SetWindowPos(
                        hwnd,
                        self.shell_view_window_handle,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                    )?;
                }
                if self.workerw_window_handle.is_some() {
                    SetWindowPos(
                        self.workerw_window_handle.unwrap(),
                        self.engine_window_handle,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                    )?;
                }
            }

            // Reparent the engine window to WorkerW
            self.selected_monitor = Some(monitor.clone());

            // Resize/reposition the engine window to match its new parent.
            // g_progmanWindowHandle spans the entire virtual desktop in modern builds

            let (edge_x, edge_y) = if static_edge_mode {
                // `WS_EX_STATICEDGE` adds borders, so we zoom in by the border thickness to mask them.
                (GetSystemMetrics(SM_CXEDGE), GetSystemMetrics(SM_CYEDGE))
            } else {
                (0, 0)
            };

            SetWindowPos(
                hwnd,
                None,
                monitor.x - edge_x,
                monitor.y - edge_y,
                monitor.width + 2 * edge_x,
                monitor.height + 2 * edge_y,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )?;

            RedrawWindow(
                self.engine_window_handle,
                None,
                None,
                RDW_INVALIDATE | RDW_UPDATENOW,
            )
            .ok()?;
        }
        Ok(())
    }

    #[inline]
    fn set_parent(child: HWND, parent: HWND) -> Result<(), WinErr> {
        unsafe { SetParent(child, Some(parent))? };
        Ok(())
    }
    /// Retrieves the target monitor information for wallpaper placement.
    ///
    /// This method determines which monitor should be used for the wallpaper window based on
    /// the provided index. The behavior depends on the index value:
    ///
    /// - **`Some(index)`**: Returns information for the specific monitor at that index.
    /// - **`None` or `Some(-1)`**: Returns the virtual desktop dimensions spanning all monitors
    ///   (using `SM_CXVIRTUALSCREEN` and `SM_CYVIRTUALSCREEN`).
    ///
    /// # Parameters
    /// - `monitor_index`: An optional index specifying which monitor to target. Use `None` or `-1`
    ///   to target the entire virtual desktop.
    ///
    /// # Returns
    /// Returns `Ok(MonitorInfo)` containing the position and dimensions of the target monitor
    /// or virtual desktop, or a [`windows::core::Error`] (Win32 error) if monitor enumeration fails.
    ///
    /// # Errors
    /// This function will return an error if:
    /// - The monitor enumeration via [`Self::enumerate_monitors`] fails.
    /// - The provided index is out of bounds (handled by falling AttachWindow to virtual desktop).
    ///
    /// # Behavior Details
    /// - If `monitor_index` is `None` or `-1`, the function returns the virtual screen dimensions
    ///   (the bounding rectangle of all monitors combined).
    /// - If a valid index is provided, the function returns the cloned `MonitorInfo` for that
    ///   specific monitor.
    /// - Invalid indices (out of range) also fall AttachWindow to the virtual desktop dimensions.
    ///
    /// # Platform Support
    /// This function is Windows-specific and relies on `GetSystemMetrics` for virtual desktop
    /// dimensions when no monitor is specified.
    ///
    /// # Safety
    /// This function contains an `unsafe` block for `GetSystemMetrics` calls. The caller should
    /// ensure that the Windows API is available and accessible.
    ///
    /// # Example
    /// ```
    /// let mut platform = AttachWindow::initialize()?;
    ///
    /// // Get the first monitor
    /// let primary_monitor = platform.get_wallpaper_target(Some(0))?;
    ///
    /// // Get the entire virtual desktop
    /// let virtual_desktop = platform.get_wallpaper_target(None)?;
    /// ```
    pub fn get_wallpaper_target(
        &mut self,
        monitor_index: Option<i32>,
    ) -> Result<MonitorInfo, WinErr> {
        let monitor_index = monitor_index.unwrap_or(-1);
        let monitors = self.enumerate_monitors()?;

        if monitor_index < 0 || monitor_index as usize >= monitors.len() {
            let mut info = MonitorInfo::default();
            info.x = 0;
            info.y = 0;
            unsafe {
                info.width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
                info.height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            }
            Ok(info)
        } else {
            Ok(monitors[monitor_index as usize].clone())
        }
    }

    // Automatically attaches the standard window handle (isize pointer) to the background.
    ///
    /// This method performs the following steps in sequence:
    /// 1. Initializes the platform AttachWindowend via [`Self::initialize`].
    /// 2. Retrieves the wallpaper target monitor using [`Self::get_wallpaper_target`] with `None`.
    /// 3. Configures the wallpaper window with the provided handle and monitor using [`Self::configure_wallpaper_window`].
    /// in other mean it does:
    ///```rust
    /// let mut window_platform = Self::initialize()?;
    /// let monitor = window_platform.get_wallpaper_target(None)?;
    /// window_platform.configure_wallpaper_window(hwnd, &monitor)?;
    /// Ok(window_platform)
    /// ```
    ///
    /// # Example
    /// ```rust
    /// let mut platform = Self::auto_attach(hwnd: isize)?;
    ///
    /// ```
    ///
    /// # Errors
    /// Returns an error if any of the initialization, monitor retrieval, or configuration steps fail.
    pub fn auto_attach(hwnd: isize, static_edge_mode: bool) -> Result<Self, WinErr> {
        let mut window_platform = Self::initialize()?;
        let monitor = window_platform.get_wallpaper_target(None)?;
        window_platform.configure_wallpaper_window(hwnd, &monitor, static_edge_mode)?;
        Ok(window_platform)
    }

    /// Enumerates all display monitors connected to the system.
    ///
    /// This method retrieves information about all available monitors using the Windows
    /// `EnumDisplayMonitors` API. The monitor coordinates are then normalized to a virtual
    /// desktop coordinate system starting at `(0, 0)`.
    ///
    /// # Process
    /// 1. Calls `EnumDisplayMonitors` to collect monitor information via a callAttachWindow.
    /// 2. Determines the minimum `x` and `y` coordinates across all monitors to find the
    ///    virtual desktop origin.
    /// 3. Subtracts the origin offset from each monitor's position, resulting in coordinates
    ///    relative to `(0, 0)`.
    ///
    /// # Returns
    /// Returns `Ok(Vec<MonitorInfo>)` containing a list of all connected monitors with
    /// normalized coordinates, or a [`windows::core::Error`] (Win32 error) if the enumeration fails.
    ///
    /// # Errors
    /// This function will return an error if:
    /// - The `EnumDisplayMonitors` Windows API call fails.
    /// - No monitors are detected (though this is rare).
    ///
    /// # Notes
    /// - The desktop origin (`desktop_x`, `desktop_y`) is updated internally to the minimum
    ///   coordinates found, representing the top-left corner of the virtual desktop.
    /// - The coordinates are normalized so that the top-leftmost monitor starts at `(0, 0)`.
    ///   This is useful for consistent positioning across multi-monitor setups.
    /// - The `MonitorInfo` struct is expected to contain `x`, `y`, `width`, and `height` fields.
    ///
    /// # Safety
    /// This function contains an `unsafe` block for the `EnumDisplayMonitors` API call.
    /// The callAttachWindow function (`monitor_enum_proc`) must be implemented correctly and
    /// handle the pointer to the vector safely.
    ///
    /// # Example
    /// ```
    /// let mut platform = AttachWindow::initialize()?;
    /// let monitors = platform.enumerate_monitors()?;
    ///
    /// for (i, monitor) in monitors.iter().enumerate() {
    ///     println!("Monitor {}: position=({}, {}), size={}x{}",
    ///              i, monitor.x, monitor.y, monitor.width, monitor.height);
    /// }
    /// ```
    pub fn enumerate_monitors(&mut self) -> Result<Vec<MonitorInfo>, WinErr> {
        let mut monitor_info_vector: Vec<MonitorInfo> = Vec::new();
        let lparam = LPARAM(&mut monitor_info_vector as *mut Vec<MonitorInfo> as isize);
        unsafe { EnumDisplayMonitors(None, None, Some(monitor_enum_proc), lparam).ok()? };

        // Convert to desktop coordinates starting at 0, 0
        self.desktop_x = i32::MAX;
        self.desktop_y = i32::MAX;

        for monitor in &monitor_info_vector {
            self.desktop_x = self.desktop_x.min(monitor.x);
            self.desktop_y = self.desktop_y.min(monitor.y);
        }

        for monitor in &mut monitor_info_vector {
            monitor.x -= self.desktop_x;
            monitor.y -= self.desktop_y;
        }

        Ok(monitor_info_vector)
    }

    /// Spawns a background thread to monitor and maintain the Z-order of the wallpaper window.
    ///
    /// Periodically checks if a `WorkerW` window has been replaced between the desktop
    /// icons (`SHELLDLL_DefView`) and the engine window. If detected, it automatically
    /// reparents the engine window to restore proper layering.
    ///
    /// This is necessary on Windows 24H2+ where the system (e.g. virtual desktop switch and switch user)
    ///  may dynamically replaces `WorkerW` windows into the desktop hierarchy.
    ///
    /// # Parameters
    /// - `watcher_delay`: Duration between each Z-order check.
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if required window handles are missing.
    pub fn start_watcher(&mut self, wathcer_delay: Duration) -> Result<(), &str> {
        if self.join_handle.is_some() {
            return Err("another watcher is working");
        }

        // Convert HWNDs to isize to bypass the Rust compiler error about passing `*mut c_void` to a thread.
        let shell_def_view = self.shell_view_window_handle.unwrap().0 as isize;
        let engine = self.engine_window_handle.unwrap().0 as isize;
        let parent = if self.is_pre_24h2 {
            self.workerw_window_handle.unwrap().0 as isize
        } else {
            self.progman_window_handle.unwrap().0 as isize
        };

        let kill_flag = Arc::new(AtomicBool::new(false));
        self.watcher_kill_flag = Some(kill_flag.clone());

        let join_handle = std::thread::spawn(move || -> Result<(), WinErr> {
            let mut previous = has_workerw_between(shell_def_view, engine);
            let engine_hwnd = HWND(engine as _);
            let parent_hwnd = HWND(parent as _);

            while !kill_flag.load(Ordering::Acquire) {
                std::thread::sleep(wathcer_delay);

                // check is there a workerW between SHEL_DefView
                let current = has_workerw_between(shell_def_view, engine);

                // if yes the engine must be reparent
                if current != previous {
                    Self::set_parent(engine_hwnd, parent_hwnd)?;

                    previous = current;
                }
            }
            Ok(())
        });
        self.join_handle = Some(join_handle);

        Ok(())
    }

    /// Retrieves the current mouse cursor position relative to the desktop.
    ///
    /// This method attempts to get the cursor position using platform-specific APIs.
    /// On Windows, it uses `GetCursorPos` (via the internal `get_relative_cursor_pos` method)
    /// to obtain the current mouse coordinates.
    ///
    /// # Returns
    /// Returns a `Vector2Platform` containing the mouse coordinates `(x, y)` in desktop
    /// pixel coordinates. If the cursor position cannot be retrieved, returns `(0.0, 0.0)`.
    ///
    /// # Behavior
    /// - On success: Returns the current cursor position as floating-point coordinates.
    /// - On failure: Returns `(0.0, 0.0)` without propagating the error.
    ///
    /// # Notes
    /// - The coordinates are relative to the virtual desktop, which may span multiple monitors.
    /// - This method silently fails (returns `(0, 0)`) rather than panicking or returning a `Result`.
    /// - The position is returned as `f32` values for compatibility with the application's
    ///   coordinate system, even though the underlying API returns integer pixels.
    ///
    /// # Platform Support
    /// This function is Windows-specific and relies on the Win32 API for cursor positioning.
    ///
    /// # Example
    /// ```
    /// let platform = AttachWindow::initialize()?;
    /// let cursor_pos = platform.get_mouse_position();
    /// println!("Mouse is at: ({}, {})", cursor_pos.x, cursor_pos.y);
    /// ```
    pub fn get_mouse_position(&self) -> Vector2Platform {
        let mut p = POINT::default();

        if self.get_relative_cursor_pos(&mut p).is_ok() {
            return Vector2Platform {
                x: p.x as _,
                y: p.y as _,
            };
        }
        Vector2Platform { x: 0_f32, y: 0_f32 }
    }

    pub fn supports_dynamic_wallpaper() -> bool {
        true
    }

    pub fn supports_multi_monitor() -> bool {
        true
    }

    /// Checks if a mouse button was just pressed (edge-triggered).
    ///
    /// Returns `true` when the button transitions from released to pressed this frame.
    ///
    /// # Button Indices
    /// | Index | Button        |
    /// |-------|---------------|
    /// | 0     | Left          |
    /// | 1     | Right         |
    /// | 2     | Middle        |
    /// | 3     | X1 (AttachWindow)     |
    /// | 4     | X2 (Forward)  |
    ///
    /// # Returns
    /// `true` if the button was just pressed, `false` otherwise.
    pub fn is_mouse_button_pressed(&self, button: i32) -> bool {
        if button < 0 || button >= 5 {
            return false;
        }
        let button = button as usize;
        self.current_mouse_state[button] && !self.previous_mouse_state[button]
    }

    /// Checks if a mouse button is currently being held down (state-triggered).
    ///
    /// Returns `true` if the button is pressed at this moment, regardless of whether
    /// it was just pressed or has been held for multiple frames.
    ///
    /// # Button Indices
    /// | Index | Button        |
    /// |-------|---------------|
    /// | 0     | Left          |
    /// | 1     | Right         |
    /// | 2     | Middle        |
    /// | 3     | X1 (AttachWindow)     |
    /// | 4     | X2 (Forward)  |
    ///
    /// # Returns
    /// `true` if the button is currently held down, `false` otherwise.
    ///
    pub fn is_mouse_button_down(&self, button: i32) -> bool {
        if button < 0 || button >= 5 {
            return false;
        }
        self.current_mouse_state[button as usize]
    }

    /// Checks if a mouse button was just released (edge-triggered).
    ///
    /// Returns `true` only when the button transitions from pressed to released
    /// in the current frame.
    ///
    /// # Button Indices
    /// | Index | Button        |
    /// |-------|---------------|
    /// | 0     | Left          |
    /// | 1     | Right         |
    /// | 2     | Middle        |
    /// | 3     | X1 (AttachWindow)     |
    /// | 4     | X2 (Forward)  |
    ///
    /// # Returns
    /// `true` if the button was just released, `false` otherwise.
    pub fn is_mouse_button_released(&self, button: i32) -> bool {
        if button < 0 || button >= 5 {
            return false;
        }
        let button = button as usize;
        !self.current_mouse_state[button] && self.previous_mouse_state[button]
    }

    /// Checks if a mouse button is currently released (not being held down).
    ///
    /// Returns `true` if the button is not pressed at this moment.
    ///
    /// # Button Indices
    /// | Index | Button        |
    /// |-------|---------------|
    /// | 0     | Left          |
    /// | 1     | Right         |
    /// | 2     | Middle        |
    /// | 3     | X1 (AttachWindow)     |
    /// | 4     | X2 (Forward)  |
    ///
    /// # Returns
    /// `true` if the button is currently released, `false` otherwise.
    pub fn is_mouse_button_up(&self, button: i32) -> bool {
        if button < 0 || button >= 5 {
            return false;
        }
        !self.current_mouse_state[button as usize]
    }

    /// Returns the y cooridiate of mouse (relative cursor position)
    pub fn get_mouse_x(&self) -> i32 {
        let mut p = POINT::default();

        if self.get_relative_cursor_pos(&mut p).is_ok() {
            return p.x;
        }
        0
    }

    /// Returns the y cooridiate of mouse (relative cursor position)
    pub fn get_mouse_y(&self) -> i32 {
        let mut p = POINT::default();

        if self.get_relative_cursor_pos(&mut p).is_ok() {
            return p.y;
        }
        0
    }

    fn get_relative_cursor_pos(&self, p: &mut POINT) -> Result<(), WinErr> {
        unsafe { GetCursorPos(p)? }

        // Convert to desktop coordinates
        p.x -= self.desktop_x;
        p.y -= self.desktop_y;

        // Convert to window coordinates
        let selected_monitor = self.selected_monitor.as_ref().ok_or(WinErr::empty())?;
        p.x -= selected_monitor.x;
        p.y -= selected_monitor.y;

        Ok(())
    }

    /// Stops the background watcher thread that monitors Z-order changes.
    ///
    /// This function sends a kill signal to the watcher thread and waits for it to
    /// terminate gracefully. It should be called during cleanup or when you no longer
    /// need Z-order monitoring.
    ///
    /// # Returns
    /// Returns `Ok(())` if the watcher thread was successfully stopped, or an error if:
    /// - The watcher was not started (returns `Err("Watcher has not yet started")`).
    /// - The thread panicked or encountered an error during shutdown.
    ///
    /// # Behavior
    /// 1. Sets the kill flag to signal the watcher thread to exit.
    /// 2. Waits for the thread to finish using `join()`.
    /// 3. Clears the internal kill flag and join handle.
    ///
    /// # Notes
    /// - This is automatically called by `cleanup()`.
    /// - If the watcher thread is not running, this function returns an error.
    /// - The function blocks until the watcher thread exits.
    ///
    /// # Example
    /// ```
    /// let mut platform = AttachWindow::initialize()?;
    /// platform.start_watcher(Duration::from_millis(100))?;
    ///
    /// // ... later ...
    ///
    /// platform.s
    pub fn stop_watcher(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(kill_flag) = &self.watcher_kill_flag {
            kill_flag.store(true, Ordering::Release);

            self.watcher_kill_flag = None;

            self.join_handle
                .take()
                .unwrap()
                .join()
                .map_err(|_| "Watcher thread panicked")??;

            Ok(())
        } else {
            Err("Watcher has not yet started".into())
        }
    }
    /// Cleans up the platform state and restores the desktop wallpaper.
    ///
    /// This method performs cleanup operations when the wallpaper engine is shutting down.
    /// It attempts to restore the original desktop wallpaper and resets all internal window handles.
    ///
    /// # Process
    /// 1. If an engine window is currently active (`engine_window_handle` is `Some`):
    ///    - Retrieves the current desktop wallpaper path via `SPI_GETDESKWALLPAPER`.
    ///    - Reapplies the wallpaper using `SPI_SETDESKWALLPAPER` to force a refresh.
    /// 2. Clears all internal window handles (`progman_window_handle`, `workerw_window_handle`,
    ///    `shell_view_window_handle`, and `engine_window_handle`).
    ///
    /// # Returns
    /// Returns `Ok(())` on success, or a [`windows::core::Error`] (Win32 error) if the
    /// wallpaper restoration fails.
    ///
    /// # Errors
    /// This function will return an error if:
    /// - The `SystemParametersInfoW` call to reapply the wallpaper fails.
    /// - The wallpaper path buffer is insufficient (though `MAX_PATH` should be adequate).
    ///
    /// # Notes
    /// - The wallpaper is only restored if an engine window was previously configured
    ///   (`engine_window_handle` is not `None`). If no engine window exists, this function
    ///   simply clears handles and returns `Ok(())`.
    /// - `MAX_PATH` (260) is used for the wallpaper path buffer, which is the standard
    ///   maximum path length in Windows.
    /// - The `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE` flags ensure the change is saved to
    ///   the registry and broadcast to all windows.
    ///
    /// # Safety
    /// This function contains `unsafe` blocks for Windows API calls. The caller must
    /// ensure that:
    /// - The process has appropriate permissions for system parameter operations.
    /// - The window handles, if present, are valid.
    ///
    /// # Example
    /// ```
    /// let mut platform = AttachWindow::initialize()?;
    /// // ... configure wallpaper ...
    ///
    /// // Clean up when shutting down
    /// platform.cleanup()?;
    /// ```
    ///
    pub fn cleanup(&mut self) -> Result<(), Box<dyn Error>> {
        let watcher_error = if self.join_handle.is_some() {
            self.stop_watcher()
        } else {
            Ok(())
        };

        const MAX_PATH: u32 = 260_u32;
        if self.engine_window_handle.is_some() {
            // Restore the desktop wallpaper
            let mut wallpaper_path = [0_u16; MAX_PATH as usize];
            unsafe {
                if SystemParametersInfoW(
                    SPI_GETDESKWALLPAPER,
                    MAX_PATH,
                    Some(wallpaper_path.as_mut_ptr() as *mut _),
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                )
                .is_ok()
                {
                    // Reapply the wallpaper to force a refresh
                    SystemParametersInfoW(
                        SPI_SETDESKWALLPAPER,
                        0,
                        Some(wallpaper_path.as_mut_ptr() as *mut _),
                        SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
                    )?;
                }
            }
        }

        self.progman_window_handle = None;
        self.workerw_window_handle = None;
        self.shell_view_window_handle = None;
        self.engine_window_handle = None;

        watcher_error
    }

    /// Captures the current mouse button states for the current frame.
    ///
    /// This method should be called once per frame to update the internal mouse state
    /// arrays. It preserves the previous frame's state and queries all 5 mouse buttons
    /// using `GetAsyncKeyState`.
    ///
    /// # After Calling
    /// - Use [`Self::is_mouse_button_pressed`] to detect edge-triggered press events.
    /// - Use `current_mouse_state[i]` directly for continuous hold detection.
    ///
    /// # Notes
    /// - The `previous_mouse_state` array is updated before querying the current state,
    ///   enabling edge detection between frames.
    /// - The method queries buttons `0` through `4` (Left, Right, Middle, X1, X2).
    ///
    /// # Safety
    /// Uses `unsafe` for `GetAsyncKeyState` API calls. Ensure the Windows API is accessible.
    ///
    /// # Example
    /// ```no_run
    /// loop {
    ///     platform.update_mouse_state();
    ///     // ... handle input
    /// }
    /// ```
    pub fn update_mouse_state(&mut self) {
        // Save previous state
        self.previous_mouse_state
            .copy_from_slice(&self.current_mouse_state);
        // Update current state
        (0..5).for_each(|i| {
            self.current_mouse_state[i] = match get_virtual_key_for_mouse_button(i) as _ {
                0 => false,
                vk => unsafe { (GetAsyncKeyState(vk) as i32 & 0x8000) != 0 },
            }
        });
    }
}

fn get_virtual_key_for_mouse_button(button: usize) -> u16 {
    match button {
        0 => VK_LBUTTON.0,
        1 => VK_RBUTTON.0,
        2 => VK_MBUTTON.0,
        3 => VK_XBUTTON1.0,
        4 => VK_XBUTTON2.0,
        _ => 0,
    }
}

impl Drop for AttachWindow {
    fn drop(&mut self) {
        let _ = self.stop_watcher();
    }
}
/// Represents display monitor information, including its full bounds and work area.
///
/// This struct holds the position and dimensions of a physical display monitor,
/// as well as its work area (the usable desktop space excluding taskbars and
/// system trays).
///
/// # Fields
/// - `x`, `y`: The position of the monitor's top-left corner in virtual desktop coordinates.
/// - `width`, `height`: The total resolution of the monitor in pixels.
/// - `work_x`, `work_y`: The top-left corner of the work area (usable space).
/// - `work_width`, `work_height`: The dimensions of the work area (usable space).
///
/// # Coordinate System
/// - Coordinates are relative to the **virtual desktop**, which may include multiple
///   monitors. The top-left corner of the primary monitor is typically `(0, 0)`.
/// - The work area is the portion of the monitor **not** occupied by the taskbar,
///   docked application bars, or system trays.
///
/// # Differences Between Monitor and Work Area
/// | Property      | Monitor                     | Work Area                         |
/// |---------------|-----------------------------|-----------------------------------|
/// | `x`, `y`      | Full screen position        | Position of usable space          |
/// | `width`, `height` | Full screen resolution   | Usable space dimensions           |
///
/// # Example
/// ```
/// let mut platform = AttachWindow::initialize()?;
/// let monitors = platform.enumerate_monitors()?;
///
/// for monitor in &monitors {
///     println!("Monitor: ({}, {}) {}x{}", monitor.x, monitor.y, monitor.width, monitor.height);
///     println!("Work area: ({}, {}) {}x{}", monitor.work_x, monitor.work_y, monitor.work_width, monitor.work_height);
/// }
/// ```
///
/// # See Also
/// - [`enumerate_monitors`](crate::AttachWindow::enumerate_monitors) for retrieving a list of monitors.
/// - [`get_wallpaper_target`](crate::AttachWindow::get_wallpaper_target) for selecting a monitor as wallpaper target.
///
///
#[derive(Debug, Default, Clone)]
pub struct MonitorInfo {
    pub x: i32, // X coordinate of the monitor's top-left corner
    pub y: i32, // Y coordinate of the monitor's top-left corner

    pub width: i32,  // Monitor width in pixels
    pub height: i32, // Monitor height in pixels

    pub work_x: i32, // Work area top-left X
    pub work_y: i32, // Work area top-left Y

    pub work_width: i32,  // Work area width
    pub work_height: i32, // Work area height
}

#[derive(Debug, Default, Clone)]
pub struct FullscreenOcclusionData {
    pub monitor: MonitorInfo,
    pub occluded_rects: Vec<RECT>,
}

// Vector2 structure to avoid engine dependency in this header
#[derive(Debug, Default, Clone)]
pub struct Vector2Platform {
    pub x: f32,
    pub y: f32,
}

pub struct DesktopHandle(pub HDESK);

impl Drop for DesktopHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_invalid() {
                let _ = CloseDesktop(self.0);
            }
        }
    }
}

pub struct Handle(pub HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_invalid() {
                let _ = CloseHandle(self.0);
            }
        }
    }
}
