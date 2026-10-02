#![windows_subsystem = "windows"]

#[cfg(not(windows))]
compile_error!("RAM Cleanup requires Windows.");

mod cleanup;
mod tray;

use std::os::windows::process::CommandExt;
use windows_sys::Win32::{System::Threading::CREATE_NO_WINDOW, UI::WindowsAndMessaging::*};

fn main() {
    let code = match std::env::args().nth(1).as_deref() {
        Some("--menu") => {
            let code = tray::menu();
            tray::menu_closed();
            code
        }
        Some("--worker") => cleanup::run(false),
        Some("--check-worker") => cleanup::run(true),
        Some("--stop") => tray::stop(),
        _ => 64,
    };
    std::process::exit(code);
}

fn hidden_command(path: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(path);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn error(message: &str) {
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            windows_sys::w!("RAM Cleanup"),
            MB_OK | MB_ICONERROR,
        );
    }
}
