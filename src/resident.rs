#![no_std]
#![no_main]
#![windows_subsystem = "windows"]

use core::{
    ffi::c_void,
    ptr::{null, null_mut},
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        LibraryLoader::{
            GetModuleFileNameW, GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32,
            LoadLibraryExW,
        },
        Memory::{HeapOptimizeResources, HeapSetInformation},
        SystemServices::{
            HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION, HEAP_OPTIMIZE_RESOURCES_INFORMATION,
        },
        Threading::*,
    },
    UI::{Input::Ime::ImmDisableIME, Shell::*, WindowsAndMessaging::*},
};

const EVENT: u32 = WM_APP + 1;
pub const MENU_CLOSED: u32 = WM_APP + 2;
// Lives on run()'s existing stack for the entire window lifetime. Avoid a
// separate writable image page for only sixteen bytes of resident state.
struct WindowState {
    taskbar_created: u32,
    menu_process: AtomicUsize,
    menu_pid: AtomicU32,
}

struct OwnedIcon(HICON);

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { DestroyIcon(self.0) };
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    unsafe { ExitProcess(1) }
}

// Supply LLVM memory intrinsics without linking a C or Rust runtime.
#[unsafe(no_mangle)]
unsafe extern "C" fn memset(dest: *mut c_void, value: i32, count: usize) -> *mut c_void {
    for i in 0..count {
        unsafe { dest.cast::<u8>().add(i).write_volatile(value as u8) };
    }
    dest
}

#[unsafe(no_mangle)]
unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, count: usize) -> *mut c_void {
    for i in 0..count {
        unsafe {
            dest.cast::<u8>()
                .add(i)
                .write_volatile(src.cast::<u8>().add(i).read_volatile())
        };
    }
    dest
}

#[unsafe(no_mangle)]
pub extern "system" fn mainCRTStartup() -> ! {
    unsafe { ExitProcess(run()) }
}

fn close_menu(state: &WindowState, kill: bool) {
    let process = state.menu_process.swap(0, Ordering::Relaxed) as HANDLE;
    state.menu_pid.store(0, Ordering::Relaxed);
    if !process.is_null() {
        unsafe {
            if kill {
                TerminateProcess(process, 0);
            }
            CloseHandle(process);
        }
    }
}

fn release_idle_pages() {
    // One self-trim after startup or user interaction. No timer, repeated sweep,
    // system-wide cleanup, hard working-set limit, or new background thread.
    let optimization = HEAP_OPTIMIZE_RESOURCES_INFORMATION {
        Version: HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION,
        Flags: 0,
    };
    unsafe {
        // Match VTD's idle handoff: decommit unused caches in this process's
        // heaps before trimming resident pages. Live allocations stay intact.
        HeapSetInformation(
            null_mut(),
            HeapOptimizeResources,
            (&optimization as *const HEAP_OPTIMIZE_RESOURCES_INFORMATION).cast(),
            core::mem::size_of_val(&optimization),
        );
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

fn disable_hidden_window_themes() -> HMODULE {
    unsafe {
        let theme = LoadLibraryExW(
            windows_sys::w!("uxtheme.dll"),
            null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if !theme.is_null()
            && let Some(function) = GetProcAddress(theme, c"SetThemeAppProperties".as_ptr().cast())
        {
            let set_properties: unsafe extern "system" fn(u32) = core::mem::transmute(function);
            set_properties(0);
        }
        theme
    }
}

fn open_menu(state: &WindowState) {
    unsafe {
        let previous = state.menu_process.load(Ordering::Relaxed) as HANDLE;
        if !previous.is_null() && WaitForSingleObject(previous, 0) == WAIT_TIMEOUT {
            return;
        }
        close_menu(state, false);
        let mut path = [0u16; 512];
        let length = GetModuleFileNameW(null_mut(), path.as_mut_ptr(), path.len() as u32) as usize;
        if length == 0 || length >= path.len() {
            return;
        }
        let Some(index) = path[..length]
            .iter()
            .rposition(|unit| *unit == b'\\' as u16)
        else {
            return;
        };
        let helper = b"ramcleanup-helper.exe\0";
        // Fixed helper name from the same protected installation directory.
        if index + 1 + helper.len() > path.len() {
            return;
        }
        for (dest, byte) in path[index + 1..].iter_mut().zip(helper) {
            *dest = *byte as u16;
        }
        let mut arguments = [0u16; 32];
        for (dest, byte) in arguments.iter_mut().zip(b"ramcleanup-helper --menu\0") {
            *dest = *byte as u16;
        }
        let mut startup: STARTUPINFOW = core::mem::zeroed();
        startup.cb = core::mem::size_of_val(&startup) as u32;
        let mut process: PROCESS_INFORMATION = core::mem::zeroed();
        if CreateProcessW(
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
        {
            CloseHandle(process.hThread);
            state
                .menu_process
                .store(process.hProcess as usize, Ordering::Relaxed);
            state.menu_pid.store(process.dwProcessId, Ordering::Relaxed);
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(l as *const CREATESTRUCTW);
            if create.lpCreateParams.is_null() {
                return 0;
            }
            SetLastError(0);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            if GetLastError() != 0 {
                return 0;
            }
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
        if pointer.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        // Only this thread owns the window. Atomics allow synchronous window
        // message reentrancy without mutable references crossing Win32 calls.
        let state = &*pointer;
        if msg != 0 && msg == state.taskbar_created {
            update(hwnd, NIM_ADD);
            release_idle_pages();
            return 0;
        }
        match msg {
            EVENT if l as u32 == WM_RBUTTONUP => {
                open_menu(state);
                0
            }
            MENU_CLOSED if w as u32 == state.menu_pid.load(Ordering::Relaxed) => {
                close_menu(state, false);
                release_idle_pages();
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                update(hwnd, NIM_DELETE);
                close_menu(state, true);
                PostQuitMessage(0);
                0
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DefWindowProcW(hwnd, msg, w, l)
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}

fn update(hwnd: HWND, operation: u32) -> bool {
    unsafe {
        let mut owned_icon = OwnedIcon(null_mut());
        let mut icon: NOTIFYICONDATAW = core::mem::zeroed();
        icon.cbSize = core::mem::size_of_val(&icon) as u32;
        icon.hWnd = hwnd;
        icon.uID = 1;
        if operation == NIM_ADD {
            icon.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            icon.uCallbackMessage = EVENT;
            owned_icon.0 = LoadImageW(
                GetModuleHandleW(null()),
                core::ptr::without_provenance(1),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                0,
            ) as HICON;
            icon.hIcon = owned_icon.0;
            if icon.hIcon.is_null() {
                return false;
            }
            core::ptr::copy_nonoverlapping(
                windows_sys::w!("RAM Cleanup"),
                icon.szTip.as_mut_ptr(),
                12,
            );
        }
        // Explorer copies the icon during this synchronous call. The temporary
        // owned icon releases its USER/GDI resources on every return path.
        // Keep no shell DLL reference in the sleeping tray either.
        let shell = LoadLibraryExW(
            windows_sys::w!("shell32.dll"),
            null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if shell.is_null() {
            return false;
        }
        let result =
            if let Some(function) = GetProcAddress(shell, c"Shell_NotifyIconW".as_ptr().cast()) {
                let notify: unsafe extern "system" fn(u32, *const NOTIFYICONDATAW) -> i32 =
                    core::mem::transmute(function);
                notify(operation, &icon) != 0
            } else {
                false
            };
        FreeLibrary(shell);
        result
    }
}

fn run() -> u32 {
    unsafe {
        let mutex = CreateMutexW(null(), 0, windows_sys::w!("Local\\RAMCleanup.Tray"));
        if mutex.is_null() {
            return 1;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);
            return 0;
        }
        let priority = MEMORY_PRIORITY_INFORMATION {
            MemoryPriority: MEMORY_PRIORITY_VERY_LOW,
        };
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessMemoryPriority,
            (&priority as *const MEMORY_PRIORITY_INFORMATION).cast(),
            core::mem::size_of_val(&priority) as u32,
        );
        ImmDisableIME(0);
        let theme = disable_hidden_window_themes();
        let state = WindowState {
            taskbar_created: RegisterWindowMessageW(windows_sys::w!("TaskbarCreated")),
            menu_process: AtomicUsize::new(0),
            menu_pid: AtomicU32::new(0),
        };
        let instance = GetModuleHandleW(null());
        let class = windows_sys::w!("RAMCleanup.Tray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            ..core::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            if !theme.is_null() {
                FreeLibrary(theme);
            }
            CloseHandle(mutex);
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
            (&state as *const WindowState).cast(),
        );
        if !theme.is_null() {
            FreeLibrary(theme);
        }
        if hwnd.is_null() || !update(hwnd, NIM_ADD) {
            if !hwnd.is_null() {
                DestroyWindow(hwnd);
            }
            CloseHandle(mutex);
            return 1;
        }
        let mut msg = core::mem::zeroed();
        release_idle_pages();
        let result = loop {
            let status = GetMessageW(&mut msg, null_mut(), 0, 0);
            if status <= 0 {
                break u32::from(status < 0);
            }
            DispatchMessageW(&msg);
        };
        // Even a WM_QUIT/error must destroy the window before its stack-owned
        // state goes away. If WM_CLOSE already destroyed it, this is harmless.
        DestroyWindow(hwnd);
        CloseHandle(mutex);
        result
    }
}
