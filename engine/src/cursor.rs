//! Native cursor ownership for a player hosted as an editor child window.
#[cfg(windows)]
use std::sync::Arc;
#[derive(Default)]
pub struct EmbeddedCursor {
    #[cfg(windows)]
    captured: std::sync::atomic::AtomicBool,
    #[cfg(windows)]
    window: std::sync::atomic::AtomicIsize,
}
impl EmbeddedCursor {
    #[cfg(windows)]
    pub fn focused(window: &winit::window::Window) -> bool {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return false;
        };
        unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::GetFocus().0 as isize == handle.hwnd.get()
                && windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
                    == windows::Win32::UI::WindowsAndMessaging::GetAncestor(
                        windows::Win32::Foundation::HWND(handle.hwnd.get() as _),
                        windows::Win32::UI::WindowsAndMessaging::GA_ROOT,
                    )
        }
    }

    #[cfg(windows)]
    pub fn capture(
        self: &Arc<Self>,
        window: &winit::window::Window,
        captured: bool,
    ) -> anyhow::Result<()> {
        use std::sync::atomic::Ordering::Relaxed;
        use windows::Win32::UI::WindowsAndMessaging::*;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
            anyhow::bail!("missing Win32 window");
        };
        if self.window.load(Relaxed) == 0 {
            let data = Arc::into_raw(Arc::clone(self)) as usize;
            if !unsafe {
                windows::Win32::UI::Shell::SetWindowSubclass(
                    windows::Win32::Foundation::HWND(handle.hwnd.get() as _),
                    Some(cursor_proc),
                    0x5743,
                    data,
                )
            }
            .as_bool()
            {
                unsafe {
                    drop(Arc::from_raw(data as *const Self));
                }
                anyhow::bail!("cannot install play cursor handler");
            }
            self.window.store(handle.hwnd.get(), Relaxed);
        }
        if captured {
            self.confine()?;
        } else if self.captured.load(Relaxed) {
            unsafe {
                ClipCursor(None)?;
            }
        }
        self.captured.store(captured, Relaxed);
        window.set_cursor_visible(!captured);
        if Self::focused(window) {
            unsafe {
                SetCursor(if captured {
                    HCURSOR::default()
                } else {
                    LoadCursorW(None, IDC_ARROW).unwrap_or_default()
                });
            }
        }

        Ok(())
    }
    #[cfg(windows)]
    pub fn confine(&self) -> anyhow::Result<()> {
        use windows::Win32::{
            Foundation::{HWND, POINT, RECT},
            Graphics::Gdi::ClientToScreen,
            UI::WindowsAndMessaging::*,
        };
        let window = HWND(self.window.load(std::sync::atomic::Ordering::Relaxed) as _);
        let mut bounds = RECT::default();
        unsafe {
            GetClientRect(window, &mut bounds)?;
        }
        let mut origin = POINT {
            x: bounds.left,
            y: bounds.top,
        };
        unsafe {
            anyhow::ensure!(
                ClientToScreen(window, &mut origin).as_bool(),
                "cannot locate play viewport"
            );
        }
        bounds.right += origin.x;
        bounds.bottom += origin.y;
        bounds.left = origin.x;
        bounds.top = origin.y;
        unsafe {
            ClipCursor(Some(&bounds))?;
        }
        Ok(())
    }
}

#[cfg(windows)]
unsafe extern "system" fn cursor_proc(
    window: windows::Win32::Foundation::HWND,
    message: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: windows::Win32::Foundation::LPARAM,
    id: usize,
    data: usize,
) -> windows::Win32::Foundation::LRESULT {
    use std::sync::atomic::Ordering::Relaxed;
    use windows::Win32::{
        Foundation::LRESULT,
        UI::{Shell::*, WindowsAndMessaging::*},
    };
    let state = unsafe { &*(data as *const EmbeddedCursor) };
    if message == WM_SETCURSOR && l.0 & 0xffff == 1 {
        unsafe {
            SetCursor(if state.captured.load(Relaxed) {
                HCURSOR::default()
            } else {
                LoadCursorW(None, IDC_ARROW).unwrap_or_default()
            });
        }
        return LRESULT(1);
    }
    if message == WM_NCDESTROY {
        if state.captured.swap(false, Relaxed) {
            let _ = unsafe { ClipCursor(None) };
        }
        state.window.store(0, Relaxed);
        unsafe {
            let _ = RemoveWindowSubclass(window, Some(cursor_proc), id);
            drop(Arc::from_raw(data as *const EmbeddedCursor));
        }
    }
    unsafe { DefSubclassProc(window, message, w, l) }
}
