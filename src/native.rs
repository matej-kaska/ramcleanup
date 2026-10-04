//! Minimal support for no_std Windows executables.
use core::ffi::c_void;
use windows_sys::Win32::System::Threading::ExitProcess;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    unsafe { ExitProcess(1) }
}

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
