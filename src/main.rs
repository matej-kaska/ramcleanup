#![no_std]
#![no_main]
#![windows_subsystem = "windows"]

mod cleanup;
mod native;
mod tray;

use windows_sys::Win32::{
    System::{Environment::GetCommandLineW, Threading::ExitProcess},
    UI::WindowsAndMessaging::*,
};

// Accept a normally quoted executable and one fixed, optionally quoted switch.
// No shell parser, dynamic argument strings or heap are needed.
fn command_is(expected: &[u8]) -> bool {
    unsafe {
        let mut cursor = GetCommandLineW();
        if cursor.is_null() {
            return false;
        }
        let mut quoted = false;
        while *cursor != 0 {
            let unit = *cursor;
            if unit == b'"' as u16 {
                quoted = !quoted;
            } else if !quoted && matches!(unit, 9 | 32) {
                break;
            }
            cursor = cursor.add(1);
        }
        if quoted {
            return false;
        }
        while matches!(*cursor, 9 | 32) {
            cursor = cursor.add(1);
        }
        let quoted = *cursor == b'"' as u16;
        if quoted {
            cursor = cursor.add(1);
        }
        for byte in expected {
            if *cursor != *byte as u16 {
                return false;
            }
            cursor = cursor.add(1);
        }
        if quoted {
            if *cursor != b'"' as u16 {
                return false;
            }
            cursor = cursor.add(1);
        }
        while matches!(*cursor, 9 | 32) {
            cursor = cursor.add(1);
        }
        *cursor == 0
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn mainCRTStartup() -> ! {
    let code = if command_is(b"--menu") {
        let code = tray::menu();
        tray::menu_closed();
        code
    } else if command_is(b"--worker") {
        cleanup::run(false)
    } else if command_is(b"--check-worker") {
        cleanup::run(true)
    } else if command_is(b"--stop") {
        tray::stop()
    } else {
        64
    };
    unsafe { ExitProcess(code as u32) }
}

fn error(message: *const u16) {
    unsafe {
        MessageBoxW(
            core::ptr::null_mut(),
            message,
            windows_sys::w!("RAM Cleanup"),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn wait_child(process: windows_sys::Win32::System::Threading::PROCESS_INFORMATION) -> u32 {
    use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
    unsafe {
        CloseHandle(process.hThread);
        let mut code = 1;
        if WaitForSingleObject(process.hProcess, INFINITE) == 0 {
            GetExitCodeProcess(process.hProcess, &mut code);
        }
        CloseHandle(process.hProcess);
        code
    }
}
