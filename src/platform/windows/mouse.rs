// AI is used to generate documentation and comments for the code.

//! Event forwarding system for capturing and forwarding input events.
//!
//! This module provides functionality to capture global mouse and keyboard events
//! using Windows low-level hooks and forward them to a specified target window
//! (typically a wallpaper window). This enables input interaction with windows
//! that may not normally receive focus or input events.
//!
//! # Overview
//!
//! The core functionality is provided by [`EventForwarder`], which:
//! - Captures global mouse and keyboard events via low-level Windows hooks
//! - Forwards events to a target window using `PostMessageW`
//! - Maintains button state tracking for accurate event forwarding
//! - Supports optional filtering by descendant window class name
//!
//! # Architecture
//!
//! 1. **Hook Thread**: Installs `WH_MOUSE_LL` and `WH_KEYBOARD_LL` and runs
//!    the Windows message loop required by the low-level hooks.
//! 2. **Event Channel**: Hook callbacks send captured events through an
//!    `mpsc` channel to the forwarding thread.
//! 3. **Forwarding Thread**: Processes captured events and forwards them to
//!    the target window using `PostMessageW`.
//! 4. **State Management**: Tracks mouse button state and maintains atomic
//!    control flags for pausing and shutting down the forwarding session.
//! 5. **Shutdown**: `WM_QUIT` is posted to the hook thread, allowing it to
//!    exit its message loop and cleanly remove the installed hooks.
//!
//! # Event Types
//!
//! The following events can be forwarded:
//! - **Mouse**: Move, Left/Right/Middle button down/up, Scroll wheel
//! - **Keyboard**: Key down and key up events
//!
//! # Usage
//!
//! ```no_run
//! use wallopino::EventForwarder;
//!
//! // Create a forwarder that captures mouse events only
//! let forwarder = EventForwarder::new(
//!     target_hwnd,
//!     Some("TargetClass"), // Optional descendant target
//!     true,  // Include mouse
//!     false, // Exclude keyboard
//! )?;
//!
//! // Start forwarding events
//! let controller = forwarder.forward_events()?;
//!
//! // Control forwarding
//! controller.pause();  // Pause event forwarding
//! controller.resume(); // Resume event forwarding
//! controller.exit();   // Stop forwarding and exit
//! ```
//!
//! # Safety
//!
//! This module contains `unsafe` code for:
//! - Setting and managing Windows hooks via `SetWindowsHookExW`
//! - Sending messages via `PostMessageW`
//! - Converting raw pointers and window handles
//!
//! Callers must provide a valid target window handle and ensure that the
//! application has the permissions required to install and use the requested
//! Windows hooks.
//!
//! # Platform Support
//!
//! This module is Windows-specific and requires:
//! - Windows 7 or later (low-level hooks are supported on Windows 7+)
//! - Appropriate permissions for global hooks

// ...
use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::{
            SystemServices::{MK_LBUTTON, MK_MBUTTON, MK_RBUTTON},
            Threading::GetCurrentThreadId,
        },
        UI::WindowsAndMessaging::{
            DispatchMessageW, GW_CHILD, GW_HWNDNEXT, GetMessageW, GetWindow, MSG, PostMessageW,
            PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
            WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN,
            WM_RBUTTONUP,
        },
    },
    core::Error as WinErr,
};

use crate::platform::windows::{
    functions::{class_and_title, create_hwnd},
    procs::{EVENT_TX, keyboard_hook, mouse_hook},
};

/// Input events that can be captured and forwarded to a target window.
///
/// Each variant represents a specific mouse or keyboard input event together
/// with the information required to recreate that event on the target window.
///
/// Mouse coordinates are represented using screen coordinates and are converted
/// to client coordinates before the corresponding Windows message is posted.
///
/// # Mouse Events
///
/// - [`Events::Move`] - Mouse cursor movement
/// - [`Events::LeftDown`] - Left mouse button pressed
/// - [`Events::LeftUp`] - Left mouse button released
/// - [`Events::RightDown`] - Right mouse button pressed
/// - [`Events::RightUp`] - Right mouse button released
/// - [`Events::MiddleDown`] - Middle mouse button pressed
/// - [`Events::MiddleUp`] - Middle mouse button released
/// - [`Events::Scroll`] - Mouse wheel movement
///
/// # Keyboard Events
///
/// - [`Events::KeyDown`] - Keyboard key pressed
/// - [`Events::KeyUp`] - Keyboard key released
///
/// # Examples
///
/// ```
/// # use wallopino::Events;
///
/// let event = Events::LeftDown { x: 100, y: 200 };
/// println!("{event:?}");
/// ```
#[derive(Debug, Clone)]
pub enum Events {
    /// Indicates that the mouse cursor moved to the specified screen position.
    ///
    /// The coordinates are converted to the target window's client coordinate
    /// system before the `WM_MOUSEMOVE` message is posted.
    Move { x: i32, y: i32 },

    /// Indicates that the left mouse button was pressed.
    LeftDown { x: i32, y: i32 },

    /// Indicates that the left mouse button was released.
    LeftUp { x: i32, y: i32 },

    /// Indicates that the right mouse button was pressed.
    RightDown { x: i32, y: i32 },

    /// Indicates that the right mouse button was released.
    RightUp { x: i32, y: i32 },

    /// Indicates that the middle mouse button was pressed.
    MiddleDown { x: i32, y: i32 },

    /// Indicates that the middle mouse button was released.
    MiddleUp { x: i32, y: i32 },

    /// Indicates that the mouse wheel was scrolled.
    ///
    /// The `delta` value follows the Windows mouse-wheel convention, where
    /// positive values represent upward scrolling and negative values
    /// represent downward scrolling.
    Scroll { x: i32, y: i32, delta: i16 },

    /// Indicates that a keyboard key was pressed.
    ///
    /// The key is identified by its Windows virtual-key code.
    KeyDown { vk: u32 },

    /// Indicates that a keyboard key was released.
    ///
    /// The key is identified by its Windows virtual-key code.
    KeyUp { vk: u32 },
}

/// Controls the state of an active event forwarding session.
///
/// A `ForwardingController` is returned by [`EventForwarder::forward_events`]
/// and provides thread-safe control over the forwarding thread.
///
/// # State
///
/// The controller maintains two atomic flags:
///
/// - `pause_flag`: Controls whether captured events are currently forwarded.
///   - `false` = forwarding is active
///   - `true` = forwarding is paused
/// - `exit_flag`: Signals that the forwarding session should terminate.
///   - `false` = forwarding session is running
///   - `true` = exit has been requested
///
/// # Behavior
///
/// Calling [`Self::pause`] temporarily disables event forwarding while keeping
/// the underlying hooks active. Calling [`Self::resume`] enables event
/// forwarding again.
///
/// Calling [`Self::exit`] permanently signals the forwarding session to stop.
/// Once exit is requested, the session cannot be resumed.
///
/// Dropping the controller also signals the forwarding thread to terminate.
///
/// # Example
///
/// ```no_run
/// let controller = forwarder.forward_events()?;
///
/// controller.pause();
/// controller.resume();
///
/// if controller.is_forwarding() {
///     println!("Forwarding is active.");
/// }
///
/// controller.exit()?;
/// # Ok::<(), windows::core::Error>(())
/// ```
#[derive(Debug)]
pub struct ForwardingController {
    pause_flag: Arc<AtomicBool>,
    exit_flag: Arc<AtomicBool>,
    hook_thread_id: u32,
    forwarding_thread_join_handle: Option<JoinHandle<Result<(), WinErr>>>,
}

impl ForwardingController {
    /// Pauses event forwarding.
    ///
    /// The installed Windows hooks remain active while forwarding is paused,
    /// but captured events are no longer processed by [`EventForwarder`].
    ///
    /// Calling this method multiple times has no additional effect.
    pub fn pause(&self) {
        self.pause_flag.store(true, Ordering::Release);
    }

    /// Resumes event forwarding.
    ///
    /// Captured events are processed and forwarded to the configured target
    /// window again.
    ///
    /// Calling this method while forwarding is already active has no additional
    /// effect.
    pub fn resume(&self) {
        self.pause_flag.store(false, Ordering::Release);
    }

    /// Returns whether the forwarding session is currently active.
    ///
    /// # Returns
    ///
    /// Returns `true` when forwarding is enabled and `false` when forwarding
    /// is currently paused.
    pub fn is_forwarding(&self) -> bool {
        !self.pause_flag.load(Ordering::Acquire)
    }

    /// Stops the event forwarding session and waits for its threads to terminate.
    ///
    /// This method signals the forwarding thread to stop processing events and
    /// sends `WM_QUIT` to the dedicated hook thread. The hook thread then exits
    /// its message loop, removes the installed Windows hooks, and terminates.
    ///
    /// The forwarding thread waits for the hook thread to finish before returning.
    ///
    /// Once this method has been called successfully, the forwarding session cannot
    /// be resumed.
    /// controller.exit()?;
    /// # Ok::<(), windows::core::Error>(())
    /// ```
    pub fn exit(mut self) -> Result<(), WinErr> {
        self.exit_flag.store(true, Ordering::Release);

        unsafe {
            // Send quit message to hook_thread
            PostThreadMessageW(self.hook_thread_id, WM_QUIT, WPARAM(0), LPARAM(0))?;
        }

        // Waiting for forwarder thread to join
        self.forwarding_thread_join_handle
            .take()
            .unwrap()
            .join()
            .map_err(|_| WinErr::empty())??;
        Ok(())
    }
}

/// Configures and manages event capture for a target window.
///
/// `EventForwarder` is responsible for receiving captured input events and
/// converting them into Windows messages targeted at a specific window.
///
/// The forwarder can optionally redirect events to a descendant window that
/// matches a specified class name.
///
/// # Configuration
///
/// The forwarder can independently enable or disable:
///
/// - Mouse event forwarding
/// - Keyboard event forwarding
///
/// Mouse button state is tracked internally so that generated Windows messages
/// contain the appropriate button-state flags.
///
/// # Example
///
/// ```no_run
/// let forwarder = EventForwarder::new(
///     hwnd,
///     Some("TargetClass"),
///     true,
///     false,
/// )?;
///
/// let controller = forwarder.forward_events()?;
/// # Ok::<(), windows::core::Error>(())
/// ```
#[derive(Debug)]
pub struct EventForwarder {
    hwnd: isize,
    button_state: u32,
    includes_mouse: bool,
    includes_keyboard: bool,
}

impl EventForwarder {
    /// Creates a new event forwarder for the specified window.
    ///
    /// If `descendants_target_classname` is provided, the function searches
    /// the target window's descendant hierarchy for a window whose class name
    /// matches the supplied value. The matching descendant becomes the actual
    /// event target.
    ///
    /// # Arguments
    ///
    /// * `hwnd` - Handle of the window that should receive forwarded events.
    /// * `descendants_target_classname` - Optional class name of a descendant
    ///   window to use as the final event target.
    /// * `includes_mouse` - Enables global mouse event capture when `true`.
    /// * `includes_keyboard` - Enables global keyboard event capture when `true`.
    ///
    /// # Returns
    ///
    /// Returns a configured [`EventForwarder`] on success.
    ///
    /// # Errors
    ///
    /// Returns a Windows error if a descendant target was requested but could
    /// not be found.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let forwarder = EventForwarder::new(
    ///     hwnd,
    ///     Some("TargetClass"),
    ///     true,
    ///     false,
    /// )?;
    /// # Ok::<(), windows::core::Error>(())
    /// ```
    pub fn new(
        mut hwnd: isize,
        descendants_target_classname: Option<&str>,
        includes_mouse: bool,
        includes_keyboard: bool,
    ) -> Result<Self, WinErr> {
        // if descendant is given, find it, otherwise use the root
        if let Some(target) = descendants_target_classname {
            hwnd = find_descendant_target(hwnd, target)
                .ok_or(WinErr::empty())?
                .0 as isize;
        }

        Ok(Self {
            hwnd,
            button_state: 0,
            includes_keyboard,
            includes_mouse,
        })
    }

    /// Starts the low-level input hooks and the event forwarding thread.
    ///
    /// A dedicated hook thread installs and owns the Windows low-level hooks and
    /// runs the required Windows message loop. Captured events are sent through an
    /// internal channel to a separate forwarding thread, which processes the events
    /// and forwards them to the target window.
    ///
    /// The returned [`ForwardingController`] can be used to pause, resume,
    /// or stop the forwarding session.
    ///
    /// # Returns
    ///
    /// Returns a [`ForwardingController`] that controls the newly created
    /// forwarding session.
    ///
    /// # Errors
    ///
    /// Returns a Windows error if the mouse or keyboard hook cannot be
    /// installed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let forwarder = EventForwarder::new(hwnd, None, true, true)?;
    /// let controller = forwarder.forward_events()?;
    ///
    /// controller.pause();
    /// controller.resume();
    /// controller.exit()?;
    /// # Ok::<(), windows::core::Error>(())
    /// ```
    pub fn forward_events(mut self) -> Result<ForwardingController, Box<dyn Error>> {
        let pause_flag = Arc::new(AtomicBool::new(false));
        let exit_flag = Arc::new(AtomicBool::new(false));

        let (rx, hook_thread_join_handle, rx_hook_thread_id) =
            start_input_hook(self.includes_mouse, self.includes_keyboard)?;

        let hook_thread_id = rx_hook_thread_id.recv().unwrap();

        let controller = ForwardingController {
            pause_flag: pause_flag.clone(),

            exit_flag: exit_flag.clone(),

            hook_thread_id,

            forwarding_thread_join_handle: Some(thread::spawn(move || -> Result<(), WinErr> {
                while !exit_flag.load(Ordering::Acquire)
                    && let Ok(event) = rx.recv()
                {
                    if !pause_flag.load(Ordering::Acquire) {
                        self.handle_events(event)?;
                    }
                }
                // if loop is finished normally by exit_flag it is okay,
                if exit_flag.load(Ordering::Acquire) {
                    hook_thread_join_handle
                        .join()
                        .map_err(|_| WinErr::empty())??;
                    return Ok(());
                }
                // if channel sends Err and broke the loop it is abnormal
                Err(WinErr::empty())
            })),
        };

        Ok(controller)
    }

    /// Converts screen coordinates into target-window client coordinates and
    /// packs them into an `LPARAM`.
    ///
    /// Windows mouse messages such as `WM_MOUSEMOVE` and `WM_LBUTTONDOWN`
    /// expect their coordinates relative to the target window's client area.
    /// This function converts the supplied screen coordinates using
    /// [`ScreenToClient`] and packs the resulting coordinates into the format
    /// expected by mouse messages.
    ///
    /// # Arguments
    ///
    /// * `hwnd` - Target window handle represented as an `isize`.
    /// * `x` - Horizontal screen coordinate.
    /// * `y` - Vertical screen coordinate.
    ///
    /// # Returns
    ///
    /// Returns an [`LPARAM`] containing the packed client coordinates.
    ///
    /// # Errors
    ///
    /// Returns a Windows error if the screen-to-client coordinate conversion
    /// fails.
    fn make_lparam(hwnd: isize, x: i32, y: i32) -> Result<LPARAM, WinErr> {
        let mut point = windows::Win32::Foundation::POINT { x, y };
        let hwnd = create_hwnd(hwnd);
        unsafe {
            // Events coordinates are screen coordinates.
            ScreenToClient(hwnd, &mut point).ok()?;
        }

        let x = point.x as u16;
        let y = point.y as u16;

        Ok(LPARAM((x as u32 | (y as u32) << 16) as isize))
    }

    /// Processes a captured input event and forwards it to the target window.
    ///
    /// Each [`Events`] variant is translated into its corresponding Windows
    /// message and posted to the configured target window using [`PostMessageW`].
    ///
    /// Mouse button state is updated before or after the corresponding message
    /// is posted so that the generated `WPARAM` accurately reflects the current
    /// button state.
    ///
    /// # Arguments
    ///
    /// * `event` - Input event captured by the low-level hook system.
    ///
    /// # Errors
    ///
    /// Returns a Windows error if generating the event coordinates or posting
    /// the message fails.
    fn handle_events(&mut self, event: Events) -> Result<(), WinErr> {
        let hwnd_isize = self.hwnd;
        let hwnd = HWND(hwnd_isize as _);

        match event {
            Events::Move { x, y } => {
                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_MOUSEMOVE,
                        WPARAM(self.button_state as usize),
                        lparam,
                    )?;
                }
            }

            Events::LeftDown { x, y } => {
                self.button_state |= MK_LBUTTON.0 as u32;

                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_LBUTTONDOWN,
                        WPARAM(self.button_state as usize),
                        lparam,
                    )?;
                }
            }

            Events::LeftUp { x, y } => {
                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_LBUTTONUP,
                        WPARAM(self.button_state as usize),
                        lparam,
                    )?;
                }

                self.button_state &= !(MK_LBUTTON.0 as u32);
            }

            Events::RightDown { x, y } => {
                self.button_state |= MK_RBUTTON.0 as u32;

                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_RBUTTONDOWN,
                        WPARAM(self.button_state as usize),
                        lparam,
                    )?;
                }
            }

            Events::RightUp { x, y } => {
                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_RBUTTONUP,
                        WPARAM(self.button_state as usize),
                        lparam,
                    )?;
                }

                self.button_state &= !(MK_RBUTTON.0 as u32);
            }

            Events::MiddleDown { x, y } => {
                self.button_state |= MK_MBUTTON.0 as u32;

                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_MBUTTONDOWN,
                        WPARAM(self.button_state as usize),
                        lparam,
                    )?;
                }
            }

            Events::MiddleUp { x, y } => {
                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_MBUTTONUP,
                        WPARAM(self.button_state as usize),
                        lparam,
                    )?;
                }

                self.button_state &= !(MK_MBUTTON.0 as u32);
            }

            Events::Scroll { x, y, delta } => {
                let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                let wparam = WPARAM(((delta as u16 as usize) << 16) | self.button_state as usize);

                unsafe {
                    PostMessageW(Some(hwnd), WM_MOUSEWHEEL, wparam, lparam)?;
                }
            }

            Events::KeyDown { vk } => unsafe {
                PostMessageW(Some(hwnd), WM_KEYDOWN, WPARAM(vk as usize), LPARAM(0))?;
            },

            Events::KeyUp { vk } => unsafe {
                PostMessageW(Some(hwnd), WM_KEYUP, WPARAM(vk as usize), LPARAM(0))?;
            },
        }

        Ok(())
    }
}

/// Starts the requested low-level Windows input hooks.
///
/// This function creates a channel used by the hook callbacks to send captured
/// [`Events`] to the forwarding thread. A dedicated thread is created to own
/// the hooks and process the Windows message loop required by low-level hooks.
///
/// Depending on the supplied flags, the function installs:
///
/// - `WH_MOUSE_LL` for mouse events
/// - `WH_KEYBOARD_LL` for keyboard events
///
/// The hooks remain installed until `WM_QUIT` is received and the message
/// loop terminates.
///
/// # Arguments
///
/// * `includes_mouse` - Installs the mouse hook when `true`.
/// * `includes_keyboard` - Installs the keyboard hook when `true`.
///
///
/// # Returns
///
/// Returns:
///
/// - a receiving channel containing captured [`Events`],
/// - a [`JoinHandle`] for the dedicated hook thread,
/// - a receiving channel used to obtain the hook thread's Windows thread ID.
///
///
/// # Errors
///
/// Returns a Windows error if one of the requested hooks cannot be installed.
///
/// # Threading
///
/// The Windows hooks and message loop run on a dedicated thread. The hook
/// callbacks send captured events through the channel rather than processing
/// them directly inside the hook callback.
fn start_input_hook(
    includes_mouse: bool,
    includes_keyboard: bool,
) -> Result<
    (
        Receiver<Events>,
        JoinHandle<Result<(), WinErr>>,
        Receiver<u32>,
    ),
    WinErr,
> {
    let (tx, rx) = mpsc::channel::<Events>();

    let (tx_hook_thread_id, rx_hook_thread_id) = mpsc::channel::<u32>();

    let join_handle = std::thread::spawn(move || -> Result<(), WinErr> {
        // Both hooks use the same thread-local sender.
        EVENT_TX.with(|slot| {
            *slot.borrow_mut() = Some(tx);
        });

        unsafe {
            // Send the hook thread ID to the caller.
            tx_hook_thread_id
                .send(GetCurrentThreadId())
                .map_err(|_| WinErr::empty())?;

            // Mouse hook
            let mouse_hook_handle = if includes_mouse {
                Some(SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(mouse_hook),
                    Some(HINSTANCE::default()),
                    0,
                )?)
            } else {
                None
            };

            // Keyboard hook
            let keyboard_hook_handle = if includes_keyboard {
                Some(SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_hook),
                    Some(HINSTANCE::default()),
                    0,
                )?)
            } else {
                None
            };

            let mut message = MSG::default();

            // Continue processing messages until WM_QUIT is received.
            while GetMessageW(&mut message, None, 0, 0).into() {
                TranslateMessage(&message).ok()?;
                DispatchMessageW(&message);
            }
            if let Some(mouse_hook_handle) = mouse_hook_handle {
                UnhookWindowsHookEx(mouse_hook_handle)?;
            }
            if let Some(keyboard_hook_handle) = keyboard_hook_handle {
                UnhookWindowsHookEx(keyboard_hook_handle)?;
            }
        }

        Ok(())
    });
    Ok((rx, join_handle, rx_hook_thread_id))
}

/// Searches the descendant hierarchy of a window for a matching class name.
///
/// The search starts at the supplied window and recursively traverses its
/// child windows until a window whose class name matches `descendant_target_name`
/// is found.
///
/// # Arguments
///
/// * `hwnd` - Root window from which the descendant search begins.
/// * `descendant_target_name` - Class name of the descendant window to locate.
///
/// # Returns
///
/// Returns the matching [`HWND`] when a descendant with the requested class
/// name is found. Returns `None` when no matching descendant exists.
fn find_descendant_target(hwnd: isize, descendant_target_name: &str) -> Option<HWND> {
    dfs(HWND(hwnd as _), descendant_target_name)
}

/// Recursively traverses a window hierarchy using depth-first search.
///
/// The current window is checked first. If its class name matches the requested
/// target, its handle is returned immediately. Otherwise, all child windows are
/// traversed recursively until a match is found.
///
/// # Arguments
///
/// * `hwnd` - Window from which the search should continue.
/// * `target_name` - Class name of the desired target window.
///
/// # Returns
///
/// Returns the matching [`HWND`] when found, otherwise returns `None`.
///
/// # Search Order
///
/// Child windows are visited in their Windows z-order using `GW_CHILD` and
/// `GW_HWNDNEXT`.
fn dfs(hwnd: HWND, target_name: &str) -> Option<HWND> {
    let current_name = class_and_title(hwnd).0;

    if current_name == target_name {
        return Some(hwnd);
    }

    unsafe {
        let mut child = GetWindow(hwnd, GW_CHILD);

        while let Ok(c) = child {
            let result = dfs(c, target_name);
            if result.is_some() {
                return result;
            }

            child = GetWindow(c, GW_HWNDNEXT);
        }
    }
    None
}
