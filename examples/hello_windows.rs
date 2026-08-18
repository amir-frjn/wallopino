use std::{
    error::Error,
    path::{Path, PathBuf},
};

use lumin_wallpaper_rs::WindowsPlatform;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::windows::WindowExtWindows,
    window::WindowBuilder,
};

use windows::Win32::Foundation::HWND;
use wry::WebViewBuilder;

fn main() -> Result<(), Box<dyn Error>> {
    let exe_dir = Path::new("./examples").canonicalize()?;

    let html_path: PathBuf = exe_dir.join("index.html");

    if !html_path.exists() {
        return Err(format!("index.html was not found:\n{}", html_path.display()).into());
    }

    let html_url = url::Url::from_file_path(&html_path)
        .map_err(|_| "Could not convert HTML path into a file URL")?
        .to_string();

    println!("HTML: {}", html_path.display());
    println!("URL: {}", html_url);

    // ------------------------------------------------------------
    // 2. Create the native Windows event loop.
    // ------------------------------------------------------------
    let event_loop = EventLoop::new();

    // ------------------------------------------------------------
    // 3. Create the native window.
    // ------------------------------------------------------------
    let window = WindowBuilder::new()
        .with_title("Rust + WebView2")
        .with_inner_size(LogicalSize::new(1000.0, 700.0))
        .with_resizable(true)
        .build(&event_loop)?;

    // ------------------------------------------------------------
    // 4. Get the native HWND.
    // ------------------------------------------------------------
    let hwnd = window.hwnd();
    WindowsPlatform::auto_attach(HWND(hwnd as _));
    println!("HWND = 0x{:X}", hwnd as usize);

    // You can pass this HWND to your own Windows code here.
    //
    // For example:
    //
    // attach_to_desktop(hwnd);
    // set_window_style(hwnd);
    // subclass_window(hwnd);
    //
    // hwnd remains the native handle of the Tao window.

    // ------------------------------------------------------------
    // 5. Create the WebView.
    // ------------------------------------------------------------
    //
    // Because index.html is loaded as a file URL, relative resources
    // such as:
    //
    //     ./style.css
    //     ./animation.json
    //
    // can be referenced from index.html.
    //
    let _webview = WebViewBuilder::new()
        .with_url(&html_url)
        .with_devtools(cfg!(debug_assertions))
        .build(&window)?;

    println!("WebView created successfully.");

    // ------------------------------------------------------------
    // 6. Run the native event loop.
    // ------------------------------------------------------------
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } if window_id == window.id() => {
                *control_flow = ControlFlow::Exit;
            }

            _ => {}
        }
    });
}
