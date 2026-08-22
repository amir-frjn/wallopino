use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
    thread::{self},
};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::SystemServices::{MK_LBUTTON, MK_MBUTTON, MK_RBUTTON},
        UI::WindowsAndMessaging::{
            DispatchMessageW, GW_CHILD, GW_HWNDNEXT, GetMessageW, GetWindow, MSG, PostMessageW,
            SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
            WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
            WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
        },
    },
    core::Error as WinErr,
};

use crate::platform::windows::procs::{MOUSE_TX, keyboard_hook, mouse_hook};

#[derive(Debug, Clone, Copy)]
pub enum Events {
    Move { x: i32, y: i32 },
    LeftDown { x: i32, y: i32 },
    LeftUp { x: i32, y: i32 },
    RightDown { x: i32, y: i32 },
    RightUp { x: i32, y: i32 },
    MiddleDown { x: i32, y: i32 },
    MiddleUp { x: i32, y: i32 },
    Scroll { x: i32, y: i32, delta: i16 },
    KeyDown { vk: u32 },
    KeyUp { vk: u32 },
}

#[derive(Debug)]
pub struct ForwardingController(Arc<Mutex<bool>>);

impl ForwardingController {
    pub fn pause(&self) {
        *self.0.lock().unwrap() = false;
    }

    pub fn resume(&self) {
        *self.0.lock().unwrap() = true;
    }

    pub fn is_resume(&self) -> bool {
        *self.0.lock().unwrap()
    }
}

#[derive(Debug)]
pub struct EventForwarder {
    hwnds: Vec<isize>,
    button_state: u32,
    event_pipeline: Receiver<Events>,
}

impl EventForwarder {
    pub fn new(
        hwnd: isize,
        add_descendants: bool,
        includes_mouse: bool,
        includes_keyboard: bool,
    ) -> Result<Self, WinErr> {
        let hwnds = if add_descendants {
            inspect_window_tree(hwnd)
        } else {
            vec![hwnd]
        };

        Ok(Self {
            hwnds,
            button_state: 0,
            event_pipeline: start_input_hook(includes_mouse, includes_keyboard)?,
        })
    }

    pub fn forward_events(mut self) -> Result<ForwardingController, WinErr> {
        let is_resume = Arc::new(Mutex::new(true));
        let controller = ForwardingController(is_resume.clone());

        thread::spawn(move || {
            while let Ok(event) = self.event_pipeline.recv() {
                if *is_resume.lock().unwrap() {
                    self.handle_events(event).unwrap();
                }
            }
        });

        Ok(controller)
    }

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

    fn handle_events(&mut self, event: Events) -> Result<(), WinErr> {
        for &hwnd_isize in &self.hwnds {
            let hwnd = create_hwnd(hwnd_isize);

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

                    let wparam =
                        WPARAM(((delta as u16 as usize) << 16) | self.button_state as usize);

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
        }
        Ok(())
    }
}

fn start_input_hook(
    includes_mouse: bool,
    includes_keyboard: bool,
) -> Result<Receiver<Events>, WinErr> {
    let (tx, rx) = mpsc::channel::<Events>();

    std::thread::spawn(move || -> Result<(), WinErr> {
        // Both hooks use the same thread-local sender.
        MOUSE_TX.with(|slot| {
            *slot.borrow_mut() = Some(tx);
        });

        unsafe {
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

    Ok(rx)
}

fn inspect_window_tree(hwnd: isize) -> Vec<isize> {
    let mut hwnds = Vec::new();
    dfs(HWND(hwnd as _), &mut hwnds);

    hwnds
}

fn dfs(hwnd: HWND, hwnds: &mut Vec<isize>) {
    hwnds.push(hwnd.0 as isize);

    unsafe {
        let mut child = GetWindow(hwnd, GW_CHILD);

        while let Ok(c) = child {
            dfs(c, hwnds);

            child = GetWindow(c, GW_HWNDNEXT);
        }
    }
}

#[inline]
fn create_hwnd(hwnd: isize) -> HWND {
    HWND(hwnd as _)
}
