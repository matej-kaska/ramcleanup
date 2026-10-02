use std::{
    ffi::c_void,
    fmt::Write,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    System::{
        Memory::SetSystemFileCacheSize,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

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
        let mut privileges: TOKEN_PRIVILEGES = std::mem::zeroed();
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

fn snapshot(log: &mut String, label: &str) {
    let mut pages = [0usize; 22];
    let status = unsafe {
        NtQuerySystemInformation(
            80,
            pages.as_mut_ptr().cast(),
            std::mem::size_of_val(&pages) as u32,
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
    let mut log = format!(
        "RAM Cleanup {}\nProfile privilege: {profile}; quota privilege: {quota}\n",
        env!("CARGO_PKG_VERSION")
    );
    let started = std::time::Instant::now();
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
        started.elapsed().as_millis()
    );
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let _ = std::fs::write(dir.join("cleanup-last.txt"), log);
    }
    failures
}
