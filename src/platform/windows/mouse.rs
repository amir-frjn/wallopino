use std::{
    cell::RefCell,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::SystemServices::{MK_LBUTTON, MK_MBUTTON, MK_RBUTTON},
        UI::WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GW_CHILD, GW_HWNDNEXT, GetMessageW, GetWindow,
            HC_ACTION, HHOOK, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostMessageW, SetWindowsHookExW,
            TranslateMessage, UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
            WM_RBUTTONUP,
        },
    },
    core::Error as WinErr,
};

#[derive(Debug, Clone, Copy)]
pub enum MouseEvent {
    Move { x: i32, y: i32 },
    LeftDown { x: i32, y: i32 },
    LeftUp { x: i32, y: i32 },
    RightDown { x: i32, y: i32 },
    RightUp { x: i32, y: i32 },
    MiddleDown { x: i32, y: i32 },
    MiddleUp { x: i32, y: i32 },
    ScrollUp { x: i32, y: i32 },
    ScrollDown { x: i32, y: i32 },
}

pub struct MouseForwarder {
    hwnds: Vec<isize>,
    button_state: u32,
    event_pipeline: Receiver<MouseEvent>,
}

impl MouseForwarder {
    pub fn new(hwnd: isize, add_descendants: bool) -> Result<Self, WinErr> {
        let hwnds = if add_descendants {
            inspect_window_tree(hwnd)
        } else {
            vec![hwnd]
        };

        Ok(Self {
            hwnds,
            button_state: 0,
            event_pipeline: start_mouse_hook()?,
        })
    }

    pub fn forward_events(mut self) -> JoinHandle<Result<(), WinErr>> {
        thread::spawn(move || -> Result<(), WinErr> {
            while let Ok(event) = self.event_pipeline.recv() {
                println!("{:?}", event);
                self.handle_mouse_event(event)?;
            }
            Ok(())
        })
    }

    fn make_lparam(hwnd: isize, x: i32, y: i32) -> Result<LPARAM, WinErr> {
        let mut point = windows::Win32::Foundation::POINT { x, y };
        let hwnd = create_hwnd(hwnd);
        unsafe {
            // MouseEvent coordinates are screen coordinates.
            ScreenToClient(hwnd, &mut point).ok()?;
        }

        let x = point.x as u16;
        let y = point.y as u16;

        Ok(LPARAM((x as u32 | (y as u32) << 16) as isize))
    }

    fn handle_mouse_event(&mut self, event: MouseEvent) -> Result<(), WinErr> {
        for &hwnd_isize in &self.hwnds {
            let hwnd = create_hwnd(hwnd_isize);

            match event {
                MouseEvent::Move { x, y } => {
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

                MouseEvent::LeftDown { x, y } => {
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

                MouseEvent::LeftUp { x, y } => {
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

                MouseEvent::RightDown { x, y } => {
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

                MouseEvent::RightUp { x, y } => {
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

                MouseEvent::MiddleDown { x, y } => {
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

                MouseEvent::MiddleUp { x, y } => {
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

                MouseEvent::ScrollUp { x, y } => {
                    let lparam = Self::make_lparam(hwnd_isize, x, y)?;

                    let delta: i16 = 120;

                    unsafe {
                        PostMessageW(
                            Some(hwnd),
                            WM_MOUSEWHEEL,
                            WPARAM(((delta as u16 as usize) << 16) | self.button_state as usize),
                            lparam,
                        )?;
                    }
                }

                MouseEvent::ScrollDown { x, y } => {
                    let lparam = Self::make_lparam(hwnd_isize, x, y)?;
                    let delta: i16 = -120;

                    unsafe {
                        PostMessageW(
                            Some(hwnd),
                            WM_MOUSEWHEEL,
                            WPARAM(((delta as u16 as usize) << 16) | self.button_state as usize),
                            lparam,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}

thread_local! {
    static MOUSE_TX: RefCell<Option<Sender<MouseEvent>>> =
        RefCell::new(None);
}

fn start_mouse_hook() -> Result<Receiver<MouseEvent>, WinErr> {
    let (tx, rx) = mpsc::channel::<MouseEvent>();

    std::thread::spawn(move || -> Result<(), WinErr> {
        MOUSE_TX.with(|slot| {
            *slot.borrow_mut() = Some(tx);
        });
        unsafe {
            let hook =
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(HINSTANCE::default()), 0)
                    .unwrap();

            let mut message = MSG::default();

            while GetMessageW(&mut message, None, 0, 0).into() {
                TranslateMessage(&message).ok()?;
                DispatchMessageW(&message);
            }

            UnhookWindowsHookEx(hook)?;
        }
        Ok(())
    });
    Ok(rx)
}

unsafe extern "system" fn mouse_hook(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= HC_ACTION as i32 {
        let (x, y, delta) = unsafe {
            let info = &*(l_param.0 as *const MSLLHOOKSTRUCT);

            if info.flags & LLMHF_INJECTED != 0 {
                return CallNextHookEx(Some(HHOOK::default()), n_code, w_param, l_param);
            }
            (info.pt.x, info.pt.y, (info.mouseData >> 16) as i16)
        };

        let event = match w_param.0 as u32 {
            WM_MOUSEMOVE => Some(MouseEvent::Move { x, y }),

            WM_LBUTTONDOWN => Some(MouseEvent::LeftDown { x, y }),

            WM_LBUTTONUP => Some(MouseEvent::LeftUp { x, y }),

            WM_RBUTTONDOWN => Some(MouseEvent::RightDown { x, y }),

            WM_RBUTTONUP => Some(MouseEvent::RightUp { x, y }),

            WM_MBUTTONDOWN => Some(MouseEvent::MiddleDown { x, y }),

            WM_MBUTTONUP => Some(MouseEvent::MiddleUp { x, y }),

            WM_MOUSEWHEEL => {
                if delta > 0 {
                    Some(MouseEvent::ScrollUp { x, y })
                } else if delta < 0 {
                    Some(MouseEvent::ScrollDown { x, y })
                } else {
                    None
                }
            }

            _ => None,
        };

        if let Some(ev) = event {
            MOUSE_TX.with(|tx| {
                if let Some(tx) = tx.borrow().as_ref() {
                    let _ = tx.send(ev);
                }
            });
        }
    }

    unsafe { CallNextHookEx(Some(HHOOK::default()), n_code, w_param, l_param) }
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

fn create_hwnd(hwnd: isize) -> HWND {
    HWND(hwnd as _)
}
