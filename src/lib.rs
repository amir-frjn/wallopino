//! Cross-platform wallpaper engine library with Windows implementation.
//!
//! This library provides platform-agnostic APIs for managing desktop wallpapers,
//! with full Windows support for attaching windows to the desktop, monitor
//! enumeration, occlusion detection, and input handling.
//!
//! # Features
//! - Desktop wallpaper attachment via Windows window hierarchy
//! - Multi-monitor support with coordinate normalization
//! - Fullscreen occlusion detection with configurable thresholds
//! - System state detection (lock screen, UAC, secure desktop)
//! - Mouse input tracking (5 buttons with edge detection)
//! - Automatic Windows version adaptation (24H2+ vs pre-24H2)
//!
//! # Quick Example
//! ```no_run
//! # use windows::Win32::Foundation::HWND;
//! # use windows::core::Error;
//! # fn example(hwnd: HWND) -> Result<(), Error> {
//! use lumin_wallpaper_rs::platform::windows::WindowsPlatform;
//!
//! let mut platform = WindowsPlatform::auto_attach(hwnd)?;
//!
//! loop {
//!     platform.update_mouse_state();
//!     if platform.is_mouse_button_pressed(0) {
//!         println!("Wallpaper clicked!");
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Platform Support
//! - ✅ Windows 10
//! - ✅ Windows 11
//! - ⚠️ Windows 8/8.1 (limited DWM features)
//!
//! # Safety
//! This crate contains `unsafe` code for Windows API calls. Callers must ensure
//! valid handles and appropriate permissions.
#[cfg(target_os = "windows")]
mod platform;
#[cfg(target_os = "windows")]
pub use platform::windows::functions::*;
#[cfg(target_os = "windows")]
pub use platform::windows::models::{MonitorInfo, Vector2Platform, WindowsPlatform};
