#[allow(dead_code, warnings, unsafe_code)]
use lumin_wallpaper_rs::WindowsPlatform;
use std::{thread::sleep, time::Duration};
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::w,
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

            let text: Vec<u16> = "Hello World from Rust HWND".encode_utf16().collect();

            TextOutW(hdc, 50, 50, &text);

            EndPaint(hwnd, &ps);

            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);

            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn create_window() -> HWND {
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();

        let class_name = w!("SimpleRustWindow");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),

            hInstance: instance,

            lpszClassName: class_name,

            ..Default::default()
        };

        RegisterClassW(&wc);

        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("Rust HWND Test"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            600,
            None,
            None,
            Some(instance),
            None,
        )
        .unwrap()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hwnd = create_window();

    unsafe {
        let mut platform = WindowsPlatform::initialize()?;
        let monitor_info = platform.get_wallpaper_target(-1)?;

        platform.configure_wallpaper_window(hwnd, &monitor_info);
        let workerw = platform.workerw_window_handle.expect("WorkerW not found");
        ShowWindow(hwnd, SW_SHOW);

        SetParent(hwnd, Some(workerw));

        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            SWP_NOZORDER | SWP_SHOWWINDOW,
        );
    }

    // 4. Existing message loop
    unsafe {
        let mut msg = MSG::default();

        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}
