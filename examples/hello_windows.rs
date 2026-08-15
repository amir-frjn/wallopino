use std::ptr::null_mut;

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
            DispatchMessageW, GetMessageW, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage,
            RegisterClassW, SW_SHOW, ShowWindow, TranslateMessage, WM_DESTROY, WNDCLASSW,
            WS_OVERLAPPEDWINDOW,
        },
    },
    core::w,
};

use lumin_wallpaper_rs::configure_wallpaper_window;

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn main() -> windows::core::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;

        let class_name = w!("LuminWallpaperTestWindow");

        let window_class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: HINSTANCE(instance.0),
            lpszClassName: class_name,
            lpfnWndProc: Some(window_proc),
            style: CS_HREDRAW | CS_VREDRAW,
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };

        RegisterClassW(&window_class);

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("LuminWallpaper - Hello World"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            600,
            None,
            None,
            Some(instance.into()),
            None,
        );

        if hwnd.0.is_null() {
            return Err(windows::core::Error::from_win32());
        }

        println!("Created test window:");
        println!("HWND = {:?}", hwnd);

        let monitors = enumerate_monitors();

        println!("Monitors:");

        for (index, monitor) in monitors.iter().enumerate() {
            println!(
                "  {}: x={}, y={}, width={}, height={}",
                index, monitor.x, monitor.y, monitor.width, monitor.height
            );
        }

        if let Some(monitor) = monitors.first() {
            println!("Configuring HWND as wallpaper...");

            configure_wallpaper_window(hwnd, monitor);
        }

        ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();

        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}
