use std::ptr::null_mut;

use lumin_wallpaper_rs::{MonitorInfo, WindowsPlatform, configure_wallpaper_window};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DrawTextW, EndPaint, PAINTSTRUCT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
            DispatchMessageW, GetMessageW, HMENU, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage,
            RegisterClassW, SW_HIDE, SW_SHOW, ShowWindow, TranslateMessage, WM_DESTROY,
            WM_ERASEBKGND, WM_PAINT, WNDCLASSW, WS_POPUP,
        },
    },
    core::{PCWSTR, w},
};

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect = ps.rcPaint;

            let mut text: Vec<u16> = "Hello Lumin-RS"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            DrawTextW(
                hdc,
                &mut text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            EndPaint(hwnd, &ps);

            LRESULT(0)
        }

        WM_ERASEBKGND => LRESULT(1),

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() -> windows::core::Result<()> {
    unsafe {
        // Initialize the Windows backend.
        let mut platform = WindowsPlatform::initialize()?;

        // -1 means the entire virtual desktop.
        let monitor: MonitorInfo = platform.get_wallpaper_target(-1_i32);

        let instance = GetModuleHandleW(None)?.into();

        let class_name = w!("LuminRsHelloWindow");

        let cursor = LoadCursorW(None, IDC_ARROW)?;

        let wc = WNDCLASSW {
            hCursor: cursor,
            hInstance: instance,
            lpszClassName: class_name,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        };

        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_win32());
        }

        // Create a normal Win32 popup window.
        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Lumin-RS"),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            monitor.width,
            monitor.height,
            None,
            HMENU(null_mut()).into(),
            instance.into(),
            None,
        )?;

        // Keep hidden while we attach it to the desktop.
        ShowWindow(hwnd, SW_HIDE);

        println!("Created HWND: {:?}", hwnd);

        // Attach the window to the Windows desktop.
        configure_wallpaper_window(hwnd, &monitor, &mut platform);
        ShowWindow(hwnd, SW_SHOW);

        // Now display it.

        println!("Wallpaper window is now attached.");
        println!("Close the process with Ctrl+C.");

        // Standard Win32 message loop.
        let mut msg = MSG::default();

        loop {
            let result = GetMessageW(&mut msg, None, 0, 0);

            if result.0 == -1 {
                return Err(windows::core::Error::from_win32());
            }

            if result.0 == 0 {
                break;
            }

            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Restore desktop state.
        platform.cleanup();

        Ok(())
    }
}
