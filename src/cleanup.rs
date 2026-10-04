use core::{
    ffi::c_void,
    fmt::Write,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    Storage::FileSystem::*,
    System::{
        LibraryLoader::GetModuleFileNameW,
        Memory::SetSystemFileCacheSize,
        SystemInformation::GetTickCount64,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

// Maximum diagnostic length is below 2 KiB even for 64-bit counters. Keep it
// on the worker's stack instead of pulling in String, fs and the std runtime.
struct Log {
    bytes: [u8; 2048],
    length: usize,
}

impl Write for Log {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let Some(end) = self.length.checked_add(text.len()) else {
            return Err(core::fmt::Error);
        };
        if end > self.bytes.len() {
            return Err(core::fmt::Error);
        }
        self.bytes[self.length..end].copy_from_slice(text.as_bytes());
        self.length = end;
        Ok(())
    }
}

#[inline(never)]
fn save_log(log: &Log) {
    unsafe {
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
        let name = b"cleanup-last.txt\0";
        if index + 1 + name.len() > path.len() {
            return;
        }
        for (dest, byte) in path[index + 1..].iter_mut().zip(name) {
            *dest = *byte as u16;
        }
        let file = CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ,
            null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        );
        if file == INVALID_HANDLE_VALUE {
            return;
        }
        let mut offset = 0;
        while offset < log.length {
            let mut written = 0;
            if WriteFile(
                file,
                log.bytes.as_ptr().add(offset),
                (log.length - offset) as u32,
                &mut written,
                null_mut(),
            ) == 0
                || written == 0
            {
                break;
            }
            offset += written as usize;
        }
        CloseHandle(file);
    }
}

// Native Windows ABI, independently implemented from the phnt definition.
// SystemMemoryListInformation = 80, commands 2, 3, 4.
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetSystemInformation(class: i32, info: *const c_void, length: u32) -> i32;
    fn NtQuerySystemInformation(
        class: i32,
        info: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

fn enable(name: *const u16) -> bool {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }
        let mut privileges: TOKEN_PRIVILEGES = core::mem::zeroed();
        privileges.PrivilegeCount = 1;
        privileges.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
        let found = LookupPrivilegeValueW(null(), name, &mut privileges.Privileges[0].Luid) != 0;
        SetLastError(0);
        let enabled = found
            && AdjustTokenPrivileges(token, 0, &privileges, 0, null_mut(), null_mut()) != 0
            && GetLastError() == 0;
        CloseHandle(token);
        enabled
    }
}

fn memory_command(command: u32) -> i32 {
    unsafe { NtSetSystemInformation(80, (&command as *const u32).cast(), 4) }
}

fn snapshot(log: &mut Log, label: &str) {
    let mut pages = [0usize; 22];
    let status = unsafe {
        NtQuerySystemInformation(
            80,
            pages.as_mut_ptr().cast(),
            core::mem::size_of_val(&pages) as u32,
            null_mut(),
        )
    };
    if status >= 0 {
        let _ = writeln!(
            log,
            "{label}: zero={} KiB free={} KiB modified={} KiB standby={} KiB",
            pages[0] * 4,
            pages[1] * 4,
            pages[2] * 4,
            pages[5..13].iter().sum::<usize>() * 4
        );
    } else {
        let _ = writeln!(log, "{label}: query NTSTATUS=0x{:08X}", status as u32);
    }
}

// Exit code is a failure bitmask: 1 working sets, 2 file cache,
// 4 modified list, 8 standby, 16 profile privilege, 32 quota privilege.
// The check mode never calls any cleanup API.
pub fn run(check: bool) -> i32 {
    let profile = enable(windows_sys::w!("SeProfileSingleProcessPrivilege"));
    let quota = enable(windows_sys::w!("SeIncreaseQuotaPrivilege"));
    let mut failures = (i32::from(!profile) * 16) | (i32::from(!quota) * 32);
    if check {
        return failures;
    }
    let mut log = Log {
        bytes: [0; 2048],
        length: 0,
    };
    let _ = writeln!(
        log,
        "RAM Cleanup {}\nProfile privilege: {profile}; quota privilege: {quota}",
        env!("CARGO_PKG_VERSION")
    );
    let started = unsafe { GetTickCount64() };
    snapshot(&mut log, "Before");
    let status = if profile {
        memory_command(2)
    } else {
        0xC0000061u32 as i32
    };
    let _ = writeln!(log, "Working sets: NTSTATUS=0x{:08X}", status as u32);
    if status < 0 {
        failures |= 1;
    }
    snapshot(&mut log, "After working sets");
    let cache = quota && unsafe { SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) } != 0;
    let _ = writeln!(
        log,
        "File cache: success={cache}, Win32 error={}",
        if cache { 0 } else { unsafe { GetLastError() } }
    );
    if !cache {
        failures |= 2;
    }
    snapshot(&mut log, "After file cache");
    let status = if profile {
        memory_command(3)
    } else {
        0xC0000061u32 as i32
    };
    let _ = writeln!(log, "Modified list: NTSTATUS=0x{:08X}", status as u32);
    if status < 0 {
        failures |= 4;
    }
    snapshot(&mut log, "After modified list");
    let status = if profile {
        memory_command(4)
    } else {
        0xC0000061u32 as i32
    };
    let _ = writeln!(log, "Standby list: NTSTATUS=0x{:08X}", status as u32);
    if status < 0 {
        failures |= 8;
    }
    snapshot(&mut log, "After standby list");
    let _ = writeln!(
        log,
        "Failure mask: {failures}; elapsed: {} ms",
        unsafe { GetTickCount64() } - started
    );
    save_log(&log);
    failures
}
