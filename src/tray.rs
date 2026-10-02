// All popup/menu caches belong to this short-lived helper, never to the tray.
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentProcessId},
    UI::{Input::Ime::ImmDisableIME, WindowsAndMessaging::*},
};

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CLOSE => {
                EndMenu();
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}

pub fn menu() -> i32 {
    unsafe {
        ImmDisableIME(0);
        let instance = GetModuleHandleW(null());
        let class = windows_sys::w!("RAMCleanup.Menu");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return 1;
        }
        let hwnd = CreateWindowExW(
            0,
            class,
            class,
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return 1;
        }
        let menu = CreatePopupMenu();
        if menu.is_null() {
            DestroyWindow(hwnd);
            return 1;
        }
        if AppendMenuW(menu, MF_STRING, 1, windows_sys::w!("Cleanup")) == 0 {
            DestroyMenu(menu);
            DestroyWindow(hwnd);
            return 1;
        }
        let mut point: POINT = std::mem::zeroed();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        );
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyWindow(hwnd);
        if command == 1 {
            return start_cleanup();
        }
    }
    0
}

fn start_cleanup() -> i32 {
    let Some(windows) = std::env::var_os("SystemRoot") else {
        return 1;
    };
    let tool = std::path::PathBuf::from(windows).join("System32/schtasks.exe");
    match super::hidden_command(tool)
        .args(["/Run", "/TN", "\\RAMCleanup.Cleanup"])
        .status()
    {
        Ok(result) if result.success() => 0,
        _ => {
            super::error("Cannot start cleanup. Install RAM Cleanup using RAMCleanup-Setup.exe.");
            1
        }
    }
}

pub fn menu_closed() {
    unsafe {
        let hwnd = FindWindowW(windows_sys::w!("RAMCleanup.Tray"), null());
        if !hwnd.is_null() {
            PostMessageW(hwnd, WM_APP + 2, GetCurrentProcessId() as WPARAM, 0);
        }
    }
}

pub fn stop() -> i32 {
    unsafe {
        let hwnd = FindWindowW(windows_sys::w!("RAMCleanup.Tray"), null());
        if !hwnd.is_null() && PostMessageW(hwnd, WM_CLOSE, 0, 0) == 0 {
            return 1;
        }
    }
    0
}
