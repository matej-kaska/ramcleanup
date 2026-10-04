// All popup/menu caches belong to this short-lived helper, never to the tray.
use core::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        LibraryLoader::GetModuleHandleW, SystemInformation::GetSystemDirectoryW, Threading::*,
    },
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
            ..core::mem::zeroed()
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
        let mut point: POINT = core::mem::zeroed();
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
    unsafe {
        let mut path = [0u16; 512];
        let length = GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) as usize;
        let suffix = b"\\schtasks.exe\0";
        let success = if length != 0 && length + suffix.len() <= path.len() {
            for (dest, byte) in path[length..].iter_mut().zip(suffix) {
                *dest = *byte as u16;
            }
            let mut arguments = [0u16; 64];
            for (dest, byte) in arguments
                .iter_mut()
                .zip(b"schtasks /Run /TN \"\\RAMCleanup.Cleanup\"\0")
            {
                *dest = *byte as u16;
            }
            let mut startup: STARTUPINFOW = core::mem::zeroed();
            startup.cb = core::mem::size_of_val(&startup) as u32;
            let mut process: PROCESS_INFORMATION = core::mem::zeroed();
            CreateProcessW(
                path.as_ptr(),
                arguments.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_NO_WINDOW,
                null(),
                null(),
                &startup,
                &mut process,
            ) != 0
                && super::wait_child(process) == 0
        } else {
            false
        };
        if success {
            0
        } else {
            super::error(windows_sys::w!(
                "Cannot start cleanup. Install RAM Cleanup using RAMCleanup-Setup.exe."
            ));
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
