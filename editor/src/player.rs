#[cfg(windows)]
mod native {
    use windows::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::*,
    };
    pub(super) struct Search {
        pub pid: u32,
        pub window: Option<HWND>,
    }
    unsafe extern "system" fn find(window: HWND, parameter: LPARAM) -> BOOL {
        let search = unsafe { &mut *(parameter.0 as *mut Search) };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut pid));
        }
        if pid == search.pid && unsafe { IsWindowVisible(window).as_bool() } {
            search.window = Some(window);
            return BOOL(0);
        }
        BOOL(1)
    }
    pub struct EmbeddedWindow {
        window: HWND,
        rect: Option<[i32; 4]>,
        visible: Option<bool>,
    }
    impl EmbeddedWindow {
        pub fn focus_editor(window: &winit::window::Window) {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::Win32(handle) = handle.as_raw()
            {
                unsafe {
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(HWND(
                        handle.hwnd.get() as _,
                    ));
                }
            }
        }
        pub fn configure(
            command: &mut std::process::Command,
            parent: &winit::window::Window,
            rect: egui::Rect,
            scale: f32,
        ) -> anyhow::Result<()> {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let RawWindowHandle::Win32(handle) = parent.window_handle()?.as_raw() else {
                anyhow::bail!("editor has no Win32 window");
            };
            let parent = HWND(handle.hwnd.get() as _);
            unsafe {
                let style = GetWindowLongPtrW(parent, GWL_STYLE);
                SetWindowLongPtrW(parent, GWL_STYLE, style | WS_CLIPCHILDREN.0 as isize);
            }
            command.env("WHATEVER_EDITOR_PARENT", handle.hwnd.get().to_string());
            command.env(
                "WHATEVER_EDITOR_RECT",
                format!(
                    "{},{},{},{}",
                    (rect.left() * scale) as i32,
                    (rect.top() * scale) as i32,
                    (rect.width() * scale).max(1.0) as i32,
                    (rect.height() * scale).max(1.0) as i32
                ),
            );
            Ok(())
        }
        pub fn attach(pid: u32, parent: &winit::window::Window) -> anyhow::Result<Option<Self>> {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let RawWindowHandle::Win32(handle) = parent.window_handle()?.as_raw() else {
                return Ok(None);
            };
            let parent = HWND(handle.hwnd.get() as _);
            let mut search = Search { pid, window: None };
            let _ = unsafe {
                EnumChildWindows(
                    parent,
                    Some(find),
                    LPARAM((&mut search as *mut Search) as isize),
                )
            };
            let Some(window) = search.window else {
                return Ok(None);
            };
            unsafe {
                anyhow::ensure!(
                    GetParent(window)? == parent,
                    "could not embed the player window"
                );
            }
            Ok(Some(Self {
                window,
                rect: None,
                visible: None,
            }))
        }
        pub fn visible(&mut self, visible: bool) {
            if self.visible == Some(visible) {
                return;
            }
            self.visible = Some(visible);
            unsafe {
                let _ = ShowWindow(self.window, if visible { SW_SHOWNA } else { SW_HIDE });
            }
        }
        pub fn layout(&mut self, rect: egui::Rect, scale: f32) {
            let next = [
                (rect.left() * scale) as i32,
                (rect.top() * scale) as i32,
                (rect.width() * scale).max(1.0) as i32,
                (rect.height() * scale).max(1.0) as i32,
            ];
            if self.rect == Some(next) {
                return;
            }
            self.rect = Some(next);
            unsafe {
                let _ = SetWindowPos(
                    self.window,
                    None,
                    (rect.left() * scale) as i32,
                    (rect.top() * scale) as i32,
                    (rect.width() * scale).max(1.0) as i32,
                    (rect.height() * scale).max(1.0) as i32,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_ASYNCWINDOWPOS | SWP_FRAMECHANGED,
                );
            }
        }
    }
}
#[cfg(windows)]
pub use native::EmbeddedWindow;

#[cfg(not(windows))]
pub struct EmbeddedWindow;
#[cfg(not(windows))]
impl EmbeddedWindow {
    pub fn attach(_: u32, _: &winit::window::Window) -> anyhow::Result<Option<Self>> {
        Ok(None)
    }
    pub fn visible(&mut self, _: bool) {}
    pub fn layout(&mut self, _: egui::Rect, _: f32) {}
}
